//! Domain-separated binary Merkle proof verification for JunoClaw state
//! commitments — mirrors `packages/app/src/app.rs::merkle_root`/`merkle_path`
//! byte-for-byte. Any divergence breaks IBC membership proofs.
//!
//! Tree layout (over the sorted non-`'_'` KV entries of committed state):
//!   leaf = sha256(0x00 || key || value)
//!   node = sha256(0x01 || left || right)   (odd nodes promote unchanged)
//!
//! The proof format carries one entry per level, bottom-up: `Some(sibling)`
//! hashes the pair, `None` promotes the node unchanged.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Hash a state leaf: `sha256(0x00 || key || value)`.
pub fn leaf_hash(key: &[u8], value: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update([0x00u8]);
    h.update(key);
    h.update(value);
    h.finalize().into()
}

/// Walk a sibling path bottom-up from `leaf` and return the computed root.
///
/// `siblings[i]` is the sibling at tree level `i`; `None` marks a promotion
/// level (odd node count — the node moves up unchanged, no hash applied).
/// `leaf_index` is the leaf's position in the sorted leaf list; its bits
/// select the hash order at each level.
pub fn compute_root(leaf: [u8; 32], leaf_index: u64, siblings: &[Option<[u8; 32]>]) -> [u8; 32] {
    let mut cur = leaf;
    let mut idx = leaf_index;
    for s in siblings {
        if let Some(sib) = s {
            let mut h = Sha256::new();
            h.update([0x01u8]);
            if idx % 2 == 0 {
                h.update(cur);
                h.update(sib);
            } else {
                h.update(sib);
                h.update(cur);
            }
            cur = h.finalize().into();
        }
        // None → promotion: cur moves up unchanged.
        idx /= 2;
    }
    cur
}

/// Mirror of `slay3rd::block::BlockPayload` for bincode deserialization.
///
/// Field order and wire types must match the node struct exactly (bincode
/// is order-sensitive, self-delimiting). `Vec<u8>`/`Vec<Vec<u8>>` decode
/// identically to the node's `Vec<u8>`/`Vec<bytes::Bytes>` — for u8 content
/// serde's `serialize_seq` and `serialize_bytes` produce the same bincode
/// wire bytes (u64 length + raw bytes).
///
/// Only `state_root` is read by the verifier; the other fields exist to
/// keep the decode aligned. Test-only: the production verifier reads the
/// trailing 32 bytes directly (state_root is the last bincode field), so
/// this struct is only needed to *build* fixture payloads in tests.
#[cfg(test)]
#[derive(Debug, Serialize, Deserialize)]
pub struct BlockPayloadMirror {
    #[allow(dead_code)]
    pub height: u64,
    #[allow(dead_code)]
    pub timestamp_nanos: u64,
    #[allow(dead_code)]
    pub proposer: Vec<u8>,
    #[allow(dead_code)]
    pub txs: Vec<Vec<u8>>,
    #[allow(dead_code)]
    pub parent_digest: [u8; 32],
    /// Merkle root over the post-state of the previous block — the
    /// commitment membership proofs verify against.
    pub state_root: [u8; 32],
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the same tree the app builds, for cross-checking proofs.
    fn build_root(leaves: &[[u8; 32]]) -> [u8; 32] {
        if leaves.is_empty() {
            return Sha256::digest([]).into();
        }
        let mut level = leaves.to_vec();
        while level.len() > 1 {
            let mut next = Vec::with_capacity((level.len() + 1) / 2);
            let mut i = 0;
            while i < level.len() {
                if i + 1 < level.len() {
                    let mut h = Sha256::new();
                    h.update([0x01u8]);
                    h.update(level[i]);
                    h.update(level[i + 1]);
                    next.push(h.finalize().into());
                    i += 2;
                } else {
                    next.push(level[i]);
                    i += 1;
                }
            }
            level = next;
        }
        level[0]
    }

    fn build_path(leaves: &[[u8; 32]], index: usize) -> Vec<Option<[u8; 32]>> {
        let mut siblings = Vec::new();
        let mut level = leaves.to_vec();
        let mut idx = index;
        while level.len() > 1 {
            let sibling = if idx % 2 == 0 { idx + 1 } else { idx - 1 };
            siblings.push(if sibling < level.len() {
                Some(level[sibling])
            } else {
                None
            });
            let mut next = Vec::with_capacity((level.len() + 1) / 2);
            let mut i = 0;
            while i < level.len() {
                if i + 1 < level.len() {
                    let mut h = Sha256::new();
                    h.update([0x01u8]);
                    h.update(level[i]);
                    h.update(level[i + 1]);
                    next.push(h.finalize().into());
                    i += 2;
                } else {
                    next.push(level[i]);
                    i += 1;
                }
            }
            level = next;
            idx /= 2;
        }
        siblings
    }

    #[test]
    fn proof_roundtrip_all_indices() {
        // Odd leaf count exercises the promotion path.
        let leaves: Vec<[u8; 32]> = (0u8..7)
            .map(|i| leaf_hash(&[b'k', i], &[b'v', i]))
            .collect();
        let root = build_root(&leaves);
        for (i, leaf) in leaves.iter().enumerate() {
            let path = build_path(&leaves, i);
            assert_eq!(compute_root(*leaf, i as u64, &path), root, "index {i}");
        }
    }

    #[test]
    fn single_leaf_tree() {
        let leaf = leaf_hash(b"only", b"leaf");
        let root = build_root(&[leaf]);
        assert_eq!(root, leaf);
        assert_eq!(compute_root(leaf, 0, &[]), root);
    }

    #[test]
    fn wrong_value_fails() {
        let leaves: Vec<[u8; 32]> = (0u8..4)
            .map(|i| leaf_hash(&[b'k', i], &[b'v', i]))
            .collect();
        let root = build_root(&leaves);
        let path = build_path(&leaves, 1);
        let bad_leaf = leaf_hash(b"k\x01", b"forged");
        assert_ne!(compute_root(bad_leaf, 1, &path), root);
    }
}
