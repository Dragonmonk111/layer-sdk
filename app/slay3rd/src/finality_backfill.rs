//! Finality-record backfill — repairs payload-only block records.
//!
//! Proposal + certificate bytes are journaled ONLY inside the
//! `Activity::Finalization` reporter path. A view that finalizes on the
//! fast-forward path (node mid-fetch when the cert forms) emits no
//! per-height Finalization activity, so execution stores the payload
//! while proposal/cert are never written — grpc.rs then serves a
//! deliberate payload-only record. On a node with persistent slow-peer
//! timing this produces a deterministic missing set (~1/4 of heights,
//! observed on the vahana seat, 2026-10-18).
//!
//! This task sweeps the store for `payload present && cert absent`
//! heights and repairs them from donor gRPCs (`layer.lightclient.v1
//! Query/Block`) — the same recovery endpoints used for state-sync.
//! Read-only for donors; local writes go through `set_block_finality`
//! (sidecar keys only — no effect on app_hash or execution).
//!
//! Donor list: `[state_sync] peers` in node.toml. Seats without that
//! section simply idle (a warning is logged once at startup).
//!
//! Throughput design: per-height cost is dominated by unreachable
//! donors, so (a) connects happen once per cycle with a cooldown for
//! donors that fail, and (b) heights fetch concurrently over cloned
//! tonic clients. A ~150k-gap store repairs in hours, not weeks.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use layer_proto::layer::lightclient::v1::{
    query_client::QueryClient as LightClientQueryClient, QueryBlockRequest,
};
use tokio::sync::RwLock;
use tonic::transport::Channel;
use tracing::{debug, info, warn};

use layer_app::App;
use layer_storage::PersistentStorage;

/// Cycle interval between sweep rounds.
const CYCLE: Duration = Duration::from_secs(60);
/// Heights scanned per cycle when catching up history (cheap local reads).
const SCAN_BATCH: u64 = 8192;
/// Heights re-checked per cycle once caught up (live-path window ~20min
/// of blocks — covers fast-forward gaps as they form).
const TAIL_WINDOW: u64 = 4096;
/// Records repaired per cycle.
const REPAIR_BATCH: usize = 256;
/// Heights fetched from donors concurrently per chunk.
const FETCH_CONCURRENCY: usize = 16;
/// Per-request donor timeout (established channels only — connects
/// use the cooldown path below).
const DONOR_TIMEOUT: Duration = Duration::from_secs(8);
/// Cycles a donor is skipped after a failed connect.
const DONOR_COOLDOWN_CYCLES: u32 = 10;
/// Cap on queued missing heights (defensive — the set is resumable).
const MISSING_CAP: usize = 1_000_000;

type ClientList = Vec<(String, LightClientQueryClient<Channel>)>;

/// Spawn the backfill task. `donors` are gRPC `host:port` endpoints
/// (the node's state-sync peer list). `app` is the shared app lock.
/// `dry_run` performs the full scan + donor fetch but never writes.
pub fn spawn<T: PersistentStorage + Send + Sync + 'static>(
    app: Arc<RwLock<App<T>>>,
    donors: Vec<String>,
    dry_run: bool,
) {
    if donors.is_empty() {
        warn!(
            "finality-backfill: no [state_sync] peers configured — \
             payload-only records will not self-repair"
        );
        return;
    }
    tokio::spawn(async move {
        // Small startup delay so the gRPC server + executor settle first.
        tokio::time::sleep(Duration::from_secs(30)).await;
        // Floor = leftmost stored payload height. A state-synced or pruned
        // store has nothing below it — scanning from height 1 would burn
        // hours of no-op reads before reaching repairable records.
        let floor = {
            let app = app.read().await;
            let tip = app.info().map(|i| i.height).unwrap_or(0);
            payload_floor(&app, tip)
        };
        info!(donors = donors.len(), dry_run, floor, "finality-backfill: started");
        let mut scan_next: u64 = floor.max(1);
        let mut missing: VecDeque<u64> = VecDeque::new();
        let mut clients: ClientList = Vec::new();
        let mut cooldown: HashMap<String, u32> = donors
            .iter()
            .map(|d| (d.clone(), 0u32))
            .collect();

        loop {
            // --- 1. advance the scan watermark, collecting gaps -----------
            {
                let tip = {
                    let app = app.read().await;
                    app.info().map(|i| i.height).unwrap_or(0)
                };
                if tip == 0 {
                    tokio::time::sleep(CYCLE).await;
                    continue;
                }
                // Once the watermark reaches tip, scan only the tail window
                // for newly-formed gaps.
                if scan_next > tip {
                    scan_next = tip.saturating_sub(TAIL_WINDOW).max(1);
                }
                let end = (scan_next + SCAN_BATCH).min(tip + 1);
                if missing.len() < MISSING_CAP {
                    let app = app.read().await;
                    for h in scan_next..end {
                        if app.get_block_payload(h).is_some()
                            && app.get_block_certificate(h).is_none()
                        {
                            missing.push_back(h);
                        }
                    }
                }
                scan_next = end;
            }

            // --- 2. (re)connect donors once per cycle, honoring cooldown ---
            for left in cooldown.values_mut() {
                if *left > 0 {
                    *left -= 1;
                }
            }
            for donor in donors.iter() {
                let left = cooldown.get(donor).copied().unwrap_or(0);
                if left > 0 {
                    continue; // cooling down after a failed connect
                }
                if clients.iter().any(|(d, _)| d == donor) {
                    continue; // already connected
                }
                let uri = format!("http://{donor}");
                match Channel::from_shared(uri) {
                    Ok(ep) => match tokio::time::timeout(DONOR_TIMEOUT, ep.connect()).await {
                        Ok(Ok(c)) => {
                            cooldown.insert(donor.clone(), 0);
                            clients.push((donor.clone(), LightClientQueryClient::new(c)));
                        }
                        _ => {
                            cooldown.insert(donor.clone(), DONOR_COOLDOWN_CYCLES);
                            warn!(donor, "finality-backfill: donor unreachable — cooling down");
                        }
                    },
                    Err(_) => {
                        cooldown.insert(donor.clone(), DONOR_COOLDOWN_CYCLES);
                        warn!(donor, "finality-backfill: invalid donor URI — cooling down");
                    }
                }
            }
            // Drop channels whose donor has gone back to cooling down.
            clients.retain(|(d, _)| cooldown.get(d).copied().unwrap_or(0) == 0);

            // --- 3. repair a bounded batch via donors ---------------------
            let batch: Vec<u64> = missing.drain(..REPAIR_BATCH.min(missing.len())).collect();
            if !batch.is_empty() {
                // Pre-fetch local payloads (the correspondence anchor) and
                // drop heights pruned mid-flight.
                let payloads: Vec<(u64, Vec<u8>)> = {
                    let app = app.read().await;
                    batch
                        .iter()
                        .filter_map(|&h| app.get_block_payload(h).map(|p| (h, p)))
                        .collect()
                };
                let mut repaired = 0u32;
                for chunk in payloads.chunks(FETCH_CONCURRENCY) {
                    let futs: Vec<_> = chunk
                        .iter()
                        .cloned()
                        .map(|(h, payload)| {
                            let clients = clients.clone();
                            async move {
                                let r = fetch_finality(&clients, h, &payload).await;
                                (h, r)
                            }
                        })
                        .collect();
                    let results = futures::future::join_all(futs).await;
                    for (h, r) in results {
                        match r {
                            Some((prop, cert, ts)) => {
                                if dry_run {
                                    info!(height = h, "finality-backfill: dry-run — would repair");
                                    repaired += 1;
                                    continue;
                                }
                                let res = {
                                    let mut app = app.write().await;
                                    app.set_block_finality(h, cert, prop, ts)
                                };
                                match res {
                                    Ok(()) => {
                                        repaired += 1;
                                        debug!(height = h, "finality-backfill: repaired");
                                    }
                                    Err(e) => {
                                        warn!(height = h, error = %e, "finality-backfill: store write failed");
                                        missing.push_back(h);
                                    }
                                }
                            }
                            None => missing.push_back(h), // no donor had it — retry next cycle
                        }
                    }
                }
                if repaired > 0 {
                    info!(
                        repaired,
                        queued = missing.len(),
                        watermark = scan_next,
                        "finality-backfill: records repaired"
                    );
                }
            }
            tokio::time::sleep(CYCLE).await;
        }
    });
}

/// Query every connected donor for a complete finality record at `height`.
///
/// A donor response counts only when ALL of these hold:
///   * `proposal_bytes` and `certificate_bytes` are both non-empty
///     (a donor's own payload-only record is a miss);
///   * `payload_bytes` byte-matches our locally executed payload — binds
///     the fetched record to the canonical chain at this height.
///
/// Returns (proposal, certificate, timestamp_nanos) for the candidate
/// backed by the most donors. A single-donor record is accepted with a
/// warn: the threshold certificate self-authenticates once written, and
/// a validated payload match leaves no room for a wrong-but-matching
/// record on an honest-majority chain.
async fn fetch_finality(
    clients: &ClientList,
    height: u64,
    local_payload: &[u8],
) -> Option<(Vec<u8>, Vec<u8>, u64)> {
    // (prop, cert, ts) -> donor count backing it
    let mut candidates: Vec<((Vec<u8>, Vec<u8>, u64), usize)> = Vec::new();
    for (donor, client) in clients {
        let resp = match tokio::time::timeout(
            DONOR_TIMEOUT,
            client.clone().block(QueryBlockRequest { height }),
        )
        .await
        {
            Ok(Ok(r)) => r.into_inner(),
            _ => continue,
        };
        if resp.proposal_bytes.is_empty() || resp.certificate_bytes.is_empty() {
            continue; // payload-only on the donor too
        }
        if resp.payload_bytes != local_payload {
            warn!(
                height,
                donor, "finality-backfill: donor record mismatches local payload — skipped"
            );
            continue;
        }
        let key = (resp.proposal_bytes, resp.certificate_bytes, resp.timestamp_nanos);
        match candidates.iter_mut().find(|(k, _)| *k == key) {
            Some((_, n)) => *n += 1,
            None => candidates.push((key, 1)),
        }
    }
    let (best, votes) = candidates.into_iter().max_by_key(|(_, n)| *n)?;
    if votes == 1 && clients.len() > 1 {
        warn!(
            height,
            "finality-backfill: single-donor record accepted (no quorum to compare)"
        );
    }
    Some(best)
}

/// Leftmost height with a stored payload, by binary search. `get_block_payload`
/// is an exact-key read, so the probe count is O(log tip) (~21 reads at 2.4M).
/// Returns 1 when nothing is stored (fresh boot — the scan then no-ops at the
/// `tip == 0` check until execution begins).
fn payload_floor<T: PersistentStorage + 'static>(app: &App<T>, tip: u64) -> u64 {
    let has = |h: u64| app.get_block_payload(h).is_some();
    let (mut lo, mut hi) = (1u64, tip);
    if !has(hi) {
        return 1;
    }
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if has(mid) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    lo
}
