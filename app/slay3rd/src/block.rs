//! BlockPayload: the application-defined block format for Commonware consensus.
//!
//! The consensus engine operates on opaque 32-byte digests. BlockPayload defines
//! the full block content — serialized with bincode for deterministic encoding.

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use sha2::{Digest as Sha2Digest, Sha256};

/// Application-defined block payload.
///
/// Serialized with bincode (deterministic for fixed-field-order structs).
/// The consensus engine only sees the 32-byte SHA-256 digest of the serialized payload.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BlockPayload {
    /// Block height (monotonically increasing, never skips)
    pub height: u64,
    /// Block timestamp in nanoseconds — from consensus context, NOT SystemTime::now()
    pub timestamp_nanos: u64,
    /// Proposer's public key bytes
    pub proposer: Vec<u8>,
    /// Ordered list of raw transaction bytes
    pub txs: Vec<Bytes>,
    /// SHA-256 digest of the parent block's payload (chain linkage)
    pub parent_digest: [u8; 32],
    /// Merkle root over the app's post-state after the PREVIOUS committed
    /// block (Tendermint app-hash semantics: the proposer cannot know its own
    /// post-state, so block H commits to the state root of H-1).
    ///
    /// Computed by `App::compute_state_root()` over all non-`'_'`-prefixed KV
    /// entries — a domain-separated binary Merkle tree
    /// (leaf = sha256(0x00 || key || value), node = sha256(0x01 || l || r)).
    /// This is what makes IBC membership proofs possible: the threshold
    /// certificate signs the payload digest, the payload carries the state
    /// root, and a Merkle path binds (key, value) to that root.
    ///
    /// Genesis block carries the root of the post-init state.
    pub state_root: [u8; 32],
}

impl BlockPayload {
    /// Serialize to deterministic bincode bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).expect("BlockPayload serialization cannot fail")
    }

    /// Deserialize from bincode bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(data)
    }

    /// Compute the SHA-256 digest of the serialized payload.
    /// This is the value used as the consensus Digest.
    pub fn digest(&self) -> [u8; 32] {
        let bytes = self.to_bytes();
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        hasher.finalize().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_payload() -> BlockPayload {
        BlockPayload {
            height: 1,
            timestamp_nanos: 1_000_000_000,
            proposer: vec![1u8; 32],
            txs: vec![
                Bytes::from("tx1"),
                Bytes::from("tx2"),
            ],
            parent_digest: [0u8; 32],
            state_root: [0u8; 32],
        }
    }

    #[test]
    fn test_block_payload_roundtrip() {
        let original = test_payload();
        let bytes = original.to_bytes();
        let decoded = BlockPayload::from_bytes(&bytes).expect("deserialization should succeed");
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_block_payload_deterministic_digest() {
        let payload = test_payload();
        let digest1 = payload.digest();
        let digest2 = payload.digest();
        assert_eq!(digest1, digest2, "digest must be deterministic");
        assert_ne!(digest1, [0u8; 32], "digest should not be all zeros");
    }

    #[test]
    fn test_different_payloads_produce_different_digests() {
        let payload1 = test_payload();
        let mut payload2 = test_payload();
        payload2.height = 2;
        assert_ne!(
            payload1.digest(),
            payload2.digest(),
            "different payloads should produce different digests"
        );
    }
}
