//! State-sync joiner — docs/STATE_SYNC.md §4.
//!
//! A fresh node (no committed state) adopts application state from a peer
//! instead of replaying history:
//!
//! 1. `ListSnapshots` — the peer's offered snapshot (latest committed
//!    state, cached server-side).
//! 2. `Block(height + 1)` on the lightclient service — the BLS-certified
//!    `BlockPayload` whose `state_root` authenticates the dump (app-hash
//!    semantics: block H+1 commits to post-H state).
//! 3. `LoadSnapshotChunk` for each chunk — the served sha256 is checked,
//!    but that is corruption detection only; the dump is authenticated
//!    by the single whole-dump Merkle root check inside
//!    `App::snapshot_import`.
//! 4. `Block(height)` supplies the timestamp written into `LAST_BLOCK`.
//!
//! Trust: the ONLY anchor is the certified `BlockPayload.state_root`.
//! Advertised snapshot metadata is untrusted.
//!
//! Certificate check: every served `Block` record is verified locally
//! (`finality::verify_finality`) against the static validator set in the
//! joiner's key file — the finalization certificate must cover the served
//! proposal and the proposal must finalize the served payload bytes. A
//! peer serving an uncertified payload is fatal Byzantine evidence.
//!
//! Multi-peer anchor: the certified payload bytes must also be served
//! byte-identically by at least `min_anchor_agree` distinct peers
//! (`StateSyncConfig`), as defense in depth.
//!
//! Download bound: the dump is held in memory until its root is checked,
//! so the joiner stops once the chunks exceed `max_snapshot_bytes`.

use anyhow::{anyhow, Context, Result};
use cosmwasm_std::Timestamp;
use sha2::{Digest as Sha2Digest, Sha256};

use layer_app::{decode_snapshot_chunk, App};
use layer_storage::PersistentStorage;

use layer_proto::layer::lightclient::v1::query_client::QueryClient as LightClientClient;
use layer_proto::layer::lightclient::v1::{QueryBlockRequest, QueryBlockResponse};
use layer_proto::layer::statesync::v1::query_client::QueryClient as StateSyncClient;
use layer_proto::layer::statesync::v1::{ListSnapshotsRequest, LoadSnapshotChunkRequest};

use crate::block::BlockPayload;
use crate::config::StateSyncConfig;
use crate::finality::FinalityCheck;

/// Hard cap on the advertised chunk count, independent of the byte cap
/// (empty chunks cost requests, not memory).
const MAX_SNAPSHOT_CHUNKS: u32 = 1 << 20;

/// Decoded snapshot fetched from a peer's statesync service.
pub struct FetchedSnapshot {
    /// Committed height the snapshot covers.
    pub height: u64,
    /// sha256-checked, fully decoded KV records (ascending keys).
    pub records: Vec<(Vec<u8>, Vec<u8>)>,
    /// The advertised state_root — must equal the certified root at
    /// `height + 1` before adoption.
    pub advertised_root: [u8; 32],
}

/// Fetch + decode the offered snapshot, trying each configured peer in
/// order until one serves a complete valid dump. All peers are donors —
/// the payload itself is untrusted until the whole-dump root check in
/// `App::snapshot_import` runs against the quorum-anchored root.
pub async fn fetch_snapshot(peers: &[String], max_bytes: u64) -> Result<FetchedSnapshot> {
    let mut errs = Vec::new();
    for peer in peers {
        match fetch_snapshot_from(peer, max_bytes).await {
            Ok(snap) => {
                tracing::info!(peer, height = snap.height,
                    "state-sync: snapshot donated");
                return Ok(snap);
            }
            Err(e) => {
                tracing::warn!(peer, error = %e,
                    "state-sync: peer failed as snapshot donor, trying next");
                errs.push(format!("{peer}: {e:#}"));
            }
        }
    }
    Err(anyhow!(
        "state-sync: no peer could donate a snapshot ({:?})",
        errs
    ))
}

/// Fetch + decode every chunk of ONE peer's offered snapshot. Each
/// chunk's sha256 is checked against the served checksum — corruption
/// detection only, NOT authentication.
///
/// The initial connect + ListSnapshots retries briefly: a joiner often
/// boots before its donor's gRPC is serving.
async fn fetch_snapshot_from(peer: &str, max_bytes: u64) -> Result<FetchedSnapshot> {
    let mut client = {
        let mut last_err = None;
        let mut c = None;
        for attempt in 1..=6 {
            match StateSyncClient::connect(format!("http://{peer}")).await {
                Ok(v) => {
                    c = Some(v);
                    break;
                }
                Err(e) => {
                    last_err = Some(e);
                    tracing::warn!(attempt, "state-sync: connect retry");
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }
        match c {
            Some(v) => v,
            None => return Err(last_err.unwrap())
                .with_context(|| format!("state-sync: connect to {peer}")),
        }
    };

    let mut snaps = None;
    for attempt in 1..=6 {
        match client.list_snapshots(ListSnapshotsRequest {}).await {
            Ok(r) => {
                snaps = Some(r.into_inner().snapshots);
                break;
            }
            Err(e) if attempt < 6 => {
                tracing::warn!(attempt, error = %e, "state-sync: ListSnapshots retry");
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            Err(e) => return Err(e).context("state-sync: ListSnapshots failed"),
        }
    }
    let snaps = snaps.expect("retry loop always sets or returns");
    // Adopt the latest offered snapshot. Height-0 (genesis) offers are
    // skipped outright: they are unanchorable — the tip anchor at
    // `snap.height` requires a certified block, and height 0 is never
    // finalized (consensus starts at height 1).
    let meta = snaps
        .into_iter()
        .filter(|m| m.height > 0)
        .max_by_key(|m| m.height)
        .ok_or_else(|| anyhow!("peer {peer} offers no usable snapshot"))?;

    anyhow::ensure!(
        meta.state_root.len() == 32,
        "snapshot meta state_root is {} bytes, expected 32",
        meta.state_root.len()
    );
    let mut advertised_root = [0u8; 32];
    advertised_root.copy_from_slice(&meta.state_root);
    anyhow::ensure!(
        meta.chunks <= MAX_SNAPSHOT_CHUNKS,
        "snapshot advertises {} chunks, more than the {MAX_SNAPSHOT_CHUNKS} cap",
        meta.chunks
    );

    let mut records = Vec::new();
    let mut total_bytes: u64 = 0;
    for i in 0..meta.chunks {
        let r = client
            .load_snapshot_chunk(LoadSnapshotChunkRequest {
                height: meta.height,
                format: meta.format,
                chunk: i,
            })
            .await
            .with_context(|| format!("state-sync: LoadSnapshotChunk {i} failed"))?
            .into_inner();
        total_bytes = total_bytes.saturating_add(r.chunk.len() as u64);
        anyhow::ensure!(
            total_bytes <= max_bytes,
            "snapshot exceeds max_snapshot_bytes ({max_bytes}) at chunk {i}"
        );
        let cksum: [u8; 32] = Sha256::digest(&r.chunk).into();
        anyhow::ensure!(
            cksum[..] == r.checksum[..],
            "snapshot chunk {i} sha256 mismatch"
        );
        records.extend(
            decode_snapshot_chunk(&r.chunk)
                .with_context(|| format!("snapshot chunk {i} decode failed"))?,
        );
    }

    Ok(FetchedSnapshot {
        height: meta.height,
        records,
        advertised_root,
    })
}

/// Query `Block(height)` from every configured peer, verify each served
/// finality record with `check`, and require the certified
/// `payload_bytes` to agree byte-for-byte on at least `min_agree`
/// distinct peers.
///
/// Unreachable peers / peers still catching up to `height` are skipped
/// (warned); a record that fails `check`, or peers that return DIFFERENT
/// payload bytes, are Byzantine evidence — the joiner aborts rather than
/// picks a winner.
async fn fetch_certified_payload(
    peers: &[String],
    height: u64,
    min_agree: usize,
    check: &FinalityCheck,
) -> Result<Vec<u8>> {
    let mut payloads: Vec<(String, Vec<u8>)> = Vec::new();
    for peer in peers {
        match fetch_block_retry(peer, height).await {
            Ok(block) => {
                check(
                    &block.proposal_bytes,
                    &block.certificate_bytes,
                    &block.payload_bytes,
                )
                .map_err(|e| {
                    anyhow!("peer {peer} served an uncertified record at height {height}: {e}")
                })?;
                payloads.push((peer.clone(), block.payload_bytes));
            }
            Err(e) => {
                tracing::warn!(peer, height, error = %e,
                    "state-sync: anchor peer did not serve block");
            }
        }
    }
    select_anchor(payloads, min_agree)
        .with_context(|| format!("state-sync: anchor quorum failed at height {height}"))
}

/// Fetch the finality record for `height` from one peer. The block is often
/// AT the donor's tip — it may still be a notarized proposal rather than
/// a finalized/certified block, so NotFound is retried briefly; other
/// errors return immediately.
async fn fetch_block_retry(peer: &str, height: u64) -> Result<QueryBlockResponse> {
    let mut lc = LightClientClient::connect(format!("http://{peer}"))
        .await
        .with_context(|| format!("lightclient connect to {peer}"))?;
    let mut last_err = None;
    for attempt in 1..=12 {
        match lc.block(QueryBlockRequest { height }).await {
            Ok(r) => return Ok(r.into_inner()),
            Err(e) if e.code() == tonic::Code::NotFound => {
                tracing::info!(peer, attempt, height,
                    "state-sync: waiting for certified block");
                last_err = Some(e);
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
            Err(e) => {
                return Err(e).with_context(|| format!("Block({height}) failed"))
            }
        }
    }
    Err(last_err.unwrap())
        .with_context(|| format!("no certified block at height {height}"))
}

/// Quorum check on the payloads peers served for one height. Every
/// respondent must agree byte-for-byte — `payload_bytes` binds height,
/// timestamp, parent_digest and state_root, so identical bytes =
/// identical anchor. Disagreement is fatal (Byzantine evidence), not a
/// vote to resolve.
fn select_anchor(payloads: Vec<(String, Vec<u8>)>, min_agree: usize) -> Result<Vec<u8>> {
    let (_, first) = match payloads.first() {
        Some(v) => v,
        None => return Err(anyhow!("no peer served the block")),
    };
    for (peer, p) in &payloads[1..] {
        anyhow::ensure!(
            p == first,
            "anchor disagreement: peer {peer} served different certified \
             payload bytes than {} — refusing to pick a winner",
            payloads[0].0
        );
    }
    anyhow::ensure!(
        payloads.len() >= min_agree,
        "anchor quorum too small: {} agreeing peers < min_anchor_agree {min_agree}",
        payloads.len()
    );
    Ok(first.clone())
}

/// Adopt a snapshot into `app` (which must be uninitialized).
///
/// The trusted root comes from the certified `BlockPayload` at
/// `snapshot_height + 1`, whose finalization certificate `check` verifies
/// against the local validator set, served identically by
/// `min_anchor_agree` peers. The advertised snapshot metadata is
/// cross-checked against it but never trusted. `chain_id` is the local
/// config's chain-id for `LAST_BLOCK`.
///
/// Returns the adopted snapshot height.
pub async fn adopt_snapshot<T>(
    app: &mut App<T>,
    ss: &StateSyncConfig,
    chain_id: &str,
    check: &FinalityCheck,
) -> Result<u64>
where
    T: PersistentStorage + Send + Sync + 'static,
{
    anyhow::ensure!(
        !ss.peers.is_empty(),
        "state-sync: configured with zero peers"
    );
    anyhow::ensure!(
        ss.min_anchor_agree >= 1 && ss.min_anchor_agree <= ss.peers.len(),
        "state-sync: min_anchor_agree {} must be in 1..={} (configured peers)",
        ss.min_anchor_agree,
        ss.peers.len()
    );

    let snap = fetch_snapshot(&ss.peers, ss.max_snapshot_bytes).await?;

    // Certified anchor: BlockPayload at height+1 commits to post-H state.
    // Must be served identically by the peer quorum.
    let anchor_bytes =
        fetch_certified_payload(&ss.peers, snap.height + 1, ss.min_anchor_agree, check).await?;
    let payload = BlockPayload::from_bytes(&anchor_bytes)
        .context("state-sync: certified payload decode failed")?;
    anyhow::ensure!(
        payload.state_root == snap.advertised_root,
        "advertised snapshot root != certified state_root at height {}",
        snap.height + 1
    );

    // Tip payload at the snapshot height — quorum-anchored identically.
    // Its timestamp field is authenticated by the quorum, unlike the
    // peer-supplied response metadata.
    let tip_bytes =
        fetch_certified_payload(&ss.peers, snap.height, ss.min_anchor_agree, check).await?;
    let tip_payload = BlockPayload::from_bytes(&tip_bytes)
        .context("state-sync: certified tip payload decode failed")?;
    anyhow::ensure!(
        tip_payload.height == snap.height,
        "certified tip payload height {} != snapshot height {}",
        tip_payload.height,
        snap.height
    );
    let block = cosmwasm_std::BlockInfo {
        height: snap.height,
        time: Timestamp::from_nanos(tip_payload.timestamp_nanos),
        chain_id: chain_id.to_string(),
    };

    app.snapshot_import(block, &snap.records, &payload.state_root, &tip_bytes)?;
    Ok(snap.height)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(bytes: &[u8]) -> (String, Vec<u8>) {
        (format!("peer-{:x?}", &bytes[..1]), bytes.to_vec())
    }

    #[test]
    fn anchor_accepts_identical_payloads() {
        let res = select_anchor(vec![p(b"aa"), p(b"aa"), p(b"aa")], 2).unwrap();
        assert_eq!(res, b"aa");
    }

    #[test]
    fn anchor_rejects_empty() {
        assert!(select_anchor(vec![], 1).is_err());
    }

    #[test]
    fn anchor_rejects_below_quorum() {
        // 2 agree but min is 3 — insufficient corroboration.
        assert!(select_anchor(vec![p(b"aa"), p(b"aa")], 3).is_err());
    }

    #[test]
    fn anchor_rejects_any_disagreement() {
        // 3-of-3 respondents but one diverges — Byzantine evidence is
        // fatal even though two agree and min_agree is only 2.
        let err = select_anchor(vec![p(b"aa"), p(b"bb"), p(b"aa")], 2)
            .unwrap_err()
            .to_string();
        assert!(err.contains("anchor disagreement"), "{err}");
    }

    #[test]
    fn anchor_single_peer_meets_quorum_of_one() {
        let res = select_anchor(vec![p(b"xy")], 1).unwrap();
        assert_eq!(res, b"xy");
    }
}
