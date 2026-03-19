//! LayerRelay: implements the Commonware `Relay` trait for broadcasting block payloads.
//!
//! CRITICAL DESIGN: LayerRelay holds the SAME `Arc<Mutex<BTreeMap<[u8; 32], BlockPayload>>>`
//! as LayerNode::pending_payloads. When the relay's `broadcast()` is called by the consensus
//! engine (after the proposer calls propose()), it ensures the payload is available in the
//! shared map so that verify() can look it up.
//!
//! In a multi-node deployment, the Relay would also serialize the BlockPayload and transmit
//! it to peer validators via commonware-p2p. For Phase 2 localhost testing, the relay is
//! in-process only — all validators share a single process via test fixtures, and the
//! pending_payloads sharing is the key mechanism.
//!
//! Phase 3+ TODO: Add actual P2P broadcast using commonware-p2p authenticated channels.
//! When a peer broadcasts a payload, the Relay receives the bytes, deserializes the
//! BlockPayload, and inserts it into the shared pending_payloads map. This is what enables
//! non-proposer validators to verify and certify proposals.

use std::collections::BTreeMap;
use std::sync::Arc;

use commonware_consensus::Relay;
use commonware_cryptography::sha256;
use tokio::sync::Mutex;
use tracing::debug;

use crate::block::BlockPayload;

/// Phase 2 in-process relay for Layer consensus.
///
/// Shares the `pending_payloads` Arc with `LayerNode` so that when the
/// consensus engine calls `broadcast()` (after the proposer's `propose()`),
/// any locally-available payload is visible to `verify()`.
///
/// The relay also maintains a separate `payload_store` for serialized payloads
/// that can be sent to peers (Phase 3 TODO: add P2P sender).
///
/// CONSTRUCTION: Always create with `LayerRelay::new(layer_node.pending_payloads(), ...)`
/// to ensure the correct Arc is shared.
#[derive(Clone)]
pub struct LayerRelay {
    /// SHARED with LayerNode::pending_payloads — same Arc instance.
    ///
    /// The proposer's `propose()` inserts payloads into this map.
    /// The relay's `broadcast()` ensures the local payload (if any) is available here.
    /// Non-proposer validators populate this map when they receive a broadcast from the relay.
    ///
    /// INVARIANT: All validators that receive a broadcast for digest D must have
    /// BlockPayload for D in this map before verify() is called for D.
    pending_payloads: Arc<Mutex<BTreeMap<[u8; 32], BlockPayload>>>,

    /// Local serialized payload store: digest -> bincode-serialized BlockPayload bytes.
    ///
    /// Used when a peer requests a specific payload by digest (Phase 3+).
    /// In Phase 2, this is populated but not consumed via P2P.
    payload_store: Arc<Mutex<BTreeMap<[u8; 32], Vec<u8>>>>,
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
    pub fn new(pending_payloads: Arc<Mutex<BTreeMap<[u8; 32], BlockPayload>>>) -> Self {
        LayerRelay {
            pending_payloads,
            payload_store: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    /// Access to the payload_store for external use (e.g., serving payloads to peers).
    pub fn payload_store(&self) -> Arc<Mutex<BTreeMap<[u8; 32], Vec<u8>>>> {
        self.payload_store.clone()
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
    /// Phase 2 (in-process only): The payload is already in pending_payloads
    /// from `propose()`. We serialize it to the payload_store for future P2P use
    /// but do not actually transmit to remote peers.
    ///
    /// Phase 3+ TODO: Send serialized bytes to all registered peers via
    /// `commonware-p2p` authenticated channels.
    async fn broadcast(&mut self, payload: Self::Digest) {
        let digest_bytes: [u8; 32] = payload.0;

        // Look up the payload from pending_payloads to serialize for the store.
        let serialized = {
            let pending = self.pending_payloads.lock().await;
            pending.get(&digest_bytes).map(|p| p.to_bytes())
        };

        if let Some(bytes) = serialized {
            let mut store = self.payload_store.lock().await;
            store.insert(digest_bytes, bytes);
            debug!(
                digest = hex::encode(digest_bytes),
                "LayerRelay: broadcast called, payload serialized to store"
            );
        } else {
            // If the payload isn't in pending_payloads, it may be a relay request
            // for a historical payload. Log for debugging.
            tracing::warn!(
                digest = hex::encode(digest_bytes),
                "LayerRelay: broadcast called for unknown digest — payload not in pending_payloads"
            );
        }

        // Phase 3+ TODO: Transmit serialized bytes to peer validators via P2P:
        //
        //   for peer in &self.peer_senders {
        //       if let Some(bytes) = payload_bytes.as_ref() {
        //           peer.send(bytes.clone()).await;
        //       }
        //   }
        //
        // On the receiving end, call `self.receive_payload(&bytes).await` to
        // insert the deserialized BlockPayload into the peer's pending_payloads.
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
        let relay = LayerRelay::new(pending.clone());
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
    fn test_broadcast_serializes_to_payload_store() {
        let (mut relay, pending_arc) = make_relay();
        rt().block_on(async {
            let payload = make_payload(1);
            let digest_bytes = payload.digest();
            let digest = sha256::Digest::from(digest_bytes);

            // Insert payload into pending_payloads first (as propose() would)
            {
                let mut pending = pending_arc.lock().await;
                pending.insert(digest_bytes, payload.clone());
            }

            // Call broadcast — should serialize to payload_store
            relay.broadcast(digest).await;

            let store = relay.payload_store.lock().await;
            assert!(store.contains_key(&digest_bytes), "payload should be in payload_store after broadcast");
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
