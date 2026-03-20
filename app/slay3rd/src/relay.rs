//! LayerRelay: implements the Commonware `Relay` trait for broadcasting block payloads.
//!
//! CRITICAL DESIGN: LayerRelay holds the SAME `Arc<Mutex<BTreeMap<[u8; 32], BlockPayload>>>`
//! as LayerNode::pending_payloads. When the relay's `broadcast()` is called by the consensus
//! engine (after the proposer calls propose()), it ensures the payload is available in the
//! shared map so that verify() can look it up.
//!
//! In a multi-process deployment, the relay must also serialize and transmit the payload to
//! peer validators via the authenticated P2P channels. The `broadcast_tx` field holds a
//! tokio mpsc sender that forwards serialized payload bytes to a background task in main.rs
//! which then calls `p2p_sender.send(Recipients::All, bytes, true)` to broadcast to peers.
//!
//! When a peer receives a payload broadcast, the background receiver task calls
//! `receive_payload()` to deserialize and insert into pending_payloads, making the
//! payload available for verify().
//!
//! Wire in main.rs:
//!   let (broadcast_tx, broadcast_rx) = tokio::sync::mpsc::unbounded_channel();
//!   let relay = LayerRelay::new(layer_node.pending_payloads(), Some(broadcast_tx));
//!   // Spawn task: reads broadcast_rx, sends via p2p_payload_sender.send(Recipients::All, ...)
//!   // Spawn task: reads p2p_payload_receiver, calls relay.receive_payload(...)

use std::collections::BTreeMap;
use std::sync::Arc;

use commonware_consensus::Relay;
use commonware_cryptography::sha256;
use tokio::sync::Mutex;
use tracing::debug;

use crate::block::BlockPayload;

/// Layer relay for authenticated cross-process consensus.
///
/// Shares the `pending_payloads` Arc with `LayerNode` so that when the
/// consensus engine calls `broadcast()` (after the proposer's `propose()`),
/// the serialized payload bytes are forwarded to a background task that
/// transmits them to peer validators via the authenticated P2P channel.
///
/// Peer validators receive the bytes via a P2P receiver task that calls
/// `receive_payload()` to insert the deserialized BlockPayload into the
/// shared pending_payloads map, enabling verify() to succeed.
///
/// CONSTRUCTION: Always create with `LayerRelay::new(layer_node.pending_payloads(), ...)`
/// to ensure the correct Arc is shared.
#[derive(Clone)]
pub struct LayerRelay {
    /// SHARED with LayerNode::pending_payloads — same Arc instance.
    ///
    /// The proposer's `propose()` inserts payloads into this map.
    /// Non-proposer validators populate this map when they receive a broadcast from the relay.
    ///
    /// INVARIANT: All validators that receive a broadcast for digest D must have
    /// BlockPayload for D in this map before verify() is called for D.
    pending_payloads: Arc<Mutex<BTreeMap<[u8; 32], BlockPayload>>>,

    /// Sender half of an unbounded channel used to forward serialized payload bytes
    /// to the background P2P broadcast task in main.rs.
    ///
    /// When `broadcast(digest)` is called by the consensus engine (proposer only),
    /// the relay serializes the payload from pending_payloads and sends the bytes here.
    /// The background task reads bytes and calls p2p_sender.send(Recipients::All, bytes).
    ///
    /// `None` when running in unit tests (in-process mode, no P2P needed).
    broadcast_tx: Option<tokio::sync::mpsc::UnboundedSender<bytes::Bytes>>,
}

impl LayerRelay {
    /// Create a new relay that shares pending_payloads with a LayerNode.
    ///
    /// IMPORTANT: `pending_payloads` MUST be the Arc returned by
    /// `LayerNode::pending_payloads()`. This is how non-proposer validators
    /// receive payloads for `verify()`.
    ///
    /// # Arguments
    ///
    /// * `pending_payloads` - The shared pending_payloads Arc from `LayerNode::pending_payloads()`
    /// * `broadcast_tx` - Optional channel sender for forwarding serialized payload bytes to
    ///   the P2P broadcast background task. Pass `None` for in-process unit tests.
    pub fn new(
        pending_payloads: Arc<Mutex<BTreeMap<[u8; 32], BlockPayload>>>,
        broadcast_tx: Option<tokio::sync::mpsc::UnboundedSender<bytes::Bytes>>,
    ) -> Self {
        LayerRelay {
            pending_payloads,
            broadcast_tx,
        }
    }

    /// Insert a received payload from a peer into the shared pending_payloads map.
    ///
    /// This is called when the relay receives a broadcast from another validator.
    /// The payload is deserialized from bytes and inserted into the map so that
    /// `verify()` can find it.
    ///
    /// Phase 3+: This method will be called from the P2P receive loop.
    pub async fn receive_payload(&self, payload_bytes: &[u8]) -> bool {
        match BlockPayload::from_bytes(payload_bytes) {
            Ok(payload) => {
                let digest = payload.digest();
                let mut pending = self.pending_payloads.lock().await;
                if !pending.contains_key(&digest) {
                    debug!(
                        height = payload.height,
                        digest = hex::encode(digest),
                        "Relay received payload from peer, inserting into pending_payloads"
                    );
                    pending.insert(digest, payload);
                }
                true
            }
            Err(e) => {
                tracing::warn!(error = %e, "Relay received malformed payload bytes from peer");
                false
            }
        }
    }
}

impl Relay for LayerRelay {
    /// The digest type matches the LayerNode's Digest type: sha256::Digest.
    type Digest = sha256::Digest;

    /// Called by the consensus engine once it decides to work on a proposal.
    ///
    /// At this point, the proposer has already inserted the payload into
    /// `pending_payloads` via `propose()`. The relay's job is to broadcast the
    /// serialized payload bytes to peer validators so they can populate their
    /// own `pending_payloads` before `verify()` is called.
    ///
    /// When `broadcast_tx` is set (multi-process mode), the serialized payload bytes
    /// are forwarded to the background P2P broadcast task which sends them to all peers
    /// via `p2p_sender.send(Recipients::All, bytes, true)`.
    ///
    /// When `broadcast_tx` is None (unit test / in-process mode), the payload is
    /// already in pending_payloads from `propose()` and is visible to all in-process
    /// validators without additional broadcast.
    async fn broadcast(&mut self, payload: Self::Digest) {
        let digest_bytes: [u8; 32] = payload.0;

        // Look up and serialize the payload from pending_payloads.
        let serialized = {
            let pending = self.pending_payloads.lock().await;
            pending.get(&digest_bytes).map(|p| p.to_bytes())
        };

        match serialized {
            Some(payload_bytes) => {
                debug!(
                    digest = hex::encode(digest_bytes),
                    bytes = payload_bytes.len(),
                    "LayerRelay: broadcasting payload to peers"
                );

                // Forward bytes to the background P2P broadcast task (if wired).
                if let Some(tx) = &self.broadcast_tx {
                    let buf = bytes::Bytes::from(payload_bytes);
                    if tx.send(buf).is_err() {
                        tracing::warn!(
                            digest = hex::encode(digest_bytes),
                            "LayerRelay: broadcast_tx channel closed — P2P broadcast skipped"
                        );
                    }
                }
            }
            None => {
                // Payload not in pending_payloads — this may happen if the proposer
                // already removed it (e.g., on re-broadcast of an old view).
                tracing::warn!(
                    digest = hex::encode(digest_bytes),
                    "LayerRelay: broadcast called for unknown digest — payload not in pending_payloads"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BlockPayload;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    fn make_relay() -> (LayerRelay, Arc<Mutex<BTreeMap<[u8; 32], BlockPayload>>>) {
        let pending = Arc::new(Mutex::new(BTreeMap::new()));
        // Pass None for broadcast_tx — in-process test mode, no P2P needed.
        let relay = LayerRelay::new(pending.clone(), None);
        (relay, pending)
    }

    fn make_payload(height: u64) -> BlockPayload {
        BlockPayload {
            height,
            timestamp_nanos: 1_000_000_000 + height * 100_000_000,
            proposer: vec![1u8; 32],
            txs: vec![],
            parent_digest: [0u8; 32],
        }
    }

    #[test]
    fn test_relay_shares_pending_payloads_arc() {
        let (relay, pending_arc) = make_relay();
        // The relay's pending_payloads should be the same Arc as we passed in
        rt().block_on(async {
            // Insert a payload via the shared arc
            let payload = make_payload(1);
            let digest = payload.digest();
            {
                let mut pending = pending_arc.lock().await;
                pending.insert(digest, payload);
            }
            // relay should also see it through its own reference
            let pending_via_relay = relay.pending_payloads.lock().await;
            assert!(pending_via_relay.contains_key(&digest));
        });
    }

    #[test]
    fn test_broadcast_sends_bytes_to_channel() {
        rt().block_on(async {
            let pending = Arc::new(Mutex::new(BTreeMap::new()));
            let (broadcast_tx, mut broadcast_rx) = tokio::sync::mpsc::unbounded_channel::<bytes::Bytes>();
            let mut relay = LayerRelay::new(pending.clone(), Some(broadcast_tx));

            let payload = make_payload(1);
            let digest_bytes = payload.digest();
            let digest = sha256::Digest::from(digest_bytes);

            // Insert payload into pending_payloads first (as propose() would)
            {
                let mut pending_lock = pending.lock().await;
                pending_lock.insert(digest_bytes, payload.clone());
            }

            // Call broadcast — should send serialized bytes to the channel
            relay.broadcast(digest).await;

            // Receive bytes from channel
            let received = broadcast_rx.try_recv();
            assert!(received.is_ok(), "broadcast should send serialized payload bytes to channel");
            assert!(!received.unwrap().is_empty(), "broadcast bytes should be non-empty");
        });
    }

    #[test]
    fn test_receive_payload_inserts_into_pending_payloads() {
        let (relay, pending_arc) = make_relay();
        rt().block_on(async {
            let payload = make_payload(2);
            let digest = payload.digest();
            let bytes = payload.to_bytes();

            // Simulate receiving from a peer
            let ok = relay.receive_payload(&bytes).await;
            assert!(ok, "receive_payload should succeed for valid bytes");

            let pending = pending_arc.lock().await;
            assert!(pending.contains_key(&digest), "payload should be in pending_payloads after receive");
        });
    }

    #[test]
    fn test_receive_invalid_payload_returns_false() {
        let (relay, _) = make_relay();
        rt().block_on(async {
            let result = relay.receive_payload(b"not-valid-bincode").await;
            assert!(!result, "invalid payload bytes should return false");
        });
    }
}
