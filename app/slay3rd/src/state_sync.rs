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

use anyhow::{anyhow, Context, Result};
use cosmwasm_std::Timestamp;
use sha2::{Digest as Sha2Digest, Sha256};

use layer_app::{decode_snapshot_chunk, App};
use layer_storage::PersistentStorage;

use layer_proto::layer::lightclient::v1::query_client::QueryClient as LightClientClient;
use layer_proto::layer::lightclient::v1::QueryBlockRequest;
use layer_proto::layer::statesync::v1::query_client::QueryClient as StateSyncClient;
use layer_proto::layer::statesync::v1::{ListSnapshotsRequest, LoadSnapshotChunkRequest};

use crate::block::BlockPayload;

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

/// Fetch + decode every chunk of the peer's offered snapshot. Each
/// chunk's sha256 is checked against the served checksum — corruption
/// detection only, NOT authentication.
///
/// The initial connect + ListSnapshots retries briefly: a joiner often
/// boots before its donor's gRPC is serving.
pub async fn fetch_snapshot(peer: &str) -> Result<FetchedSnapshot> {
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
    let meta = snaps
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("peer {peer} offers no snapshot"))?;

    anyhow::ensure!(
        meta.state_root.len() == 32,
        "snapshot meta state_root is {} bytes, expected 32",
        meta.state_root.len()
    );
    let mut advertised_root = [0u8; 32];
    advertised_root.copy_from_slice(&meta.state_root);

    let mut records = Vec::new();
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

/// Adopt the peer's snapshot into `app` (which must be uninitialized).
///
/// The trusted root comes from the BLS-certified `BlockPayload` at
/// `snapshot_height + 1` — the advertised metadata is cross-checked
/// against it but never trusted. `chain_id` is the local config's
/// chain-id for `LAST_BLOCK`.
///
/// Returns the adopted snapshot height.
pub async fn adopt_snapshot<T>(
    app: &mut App<T>,
    peer: &str,
    chain_id: &str,
) -> Result<u64>
where
    T: PersistentStorage + Send + Sync + 'static,
{
    let snap = fetch_snapshot(peer).await?;

    // Certified anchor: BlockPayload at height+1 commits to post-H state.
    let mut lc = LightClientClient::connect(format!("http://{peer}"))
        .await
        .with_context(|| format!("state-sync: lightclient connect to {peer}"))?;

    // The snapshot is often AT the donor's tip — block H+1 may still be
    // a notarized proposal, not yet a finalized/certified block. Retry
    // NotFound briefly; non-NotFound errors are fatal immediately.
    let cert = {
        let mut c = None;
        let mut last_err = None;
        for attempt in 1..=12 {
            match lc
                .block(QueryBlockRequest {
                    height: snap.height + 1,
                })
                .await
            {
                Ok(r) => {
                    c = Some(r.into_inner());
                    break;
                }
                Err(e) if e.code() == tonic::Code::NotFound => {
                    tracing::info!(attempt, height = snap.height + 1,
                        "state-sync: waiting for certified block");
                    last_err = Some(e);
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
                Err(e) => {
                    return Err(e)
                        .with_context(|| format!("state-sync: Block({}) failed", snap.height + 1));
                }
            }
        }
        match c {
            Some(v) => v,
            None => return Err(last_err.unwrap()).with_context(|| {
                format!("state-sync: no certified block at {}", snap.height + 1)
            }),
        }
    };
    let payload = BlockPayload::from_bytes(&cert.payload_bytes)
        .context("state-sync: certified payload decode failed")?;
    anyhow::ensure!(
        payload.state_root == snap.advertised_root,
        "advertised snapshot root != certified state_root at height {}",
        snap.height + 1
    );

    // Timestamp + tip payload bytes come from the certified block at the
    // snapshot height itself — also subject to the same tip race as
    // Block(H+1), so retry NotFound identically.
    let tip = {
        let mut c = None;
        let mut last_err = None;
        for attempt in 1..=12 {
            match lc
                .block(QueryBlockRequest {
                    height: snap.height,
                })
                .await
            {
                Ok(r) => {
                    c = Some(r.into_inner());
                    break;
                }
                Err(e) if e.code() == tonic::Code::NotFound => {
                    tracing::info!(attempt, height = snap.height,
                        "state-sync: waiting for certified tip block");
                    last_err = Some(e);
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
                Err(e) => {
                    return Err(e)
                        .with_context(|| format!("state-sync: Block({}) failed", snap.height));
                }
            }
        }
        match c {
            Some(v) => v,
            None => return Err(last_err.unwrap()).with_context(|| {
                format!("state-sync: certified tip payload unavailable at height {}", snap.height)
            }),
        }
    };
    let tip_payload = BlockPayload::from_bytes(&tip.payload_bytes)
        .context("state-sync: certified tip payload decode failed")?;
    anyhow::ensure!(
        tip_payload.height == snap.height,
        "certified tip payload height {} != snapshot height {}",
        tip_payload.height,
        snap.height
    );
    let block = cosmwasm_std::BlockInfo {
        height: snap.height,
        time: Timestamp::from_nanos(tip.timestamp_nanos),
        chain_id: chain_id.to_string(),
    };

    app.snapshot_import(block, &snap.records, &payload.state_root, &tip.payload_bytes)?;
    Ok(snap.height)
}
