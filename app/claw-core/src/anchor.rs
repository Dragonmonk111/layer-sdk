//! JC-TRANSCRIPT-ANCHOR-v1 — Merkle anchoring for channel transcripts.
//!
//! Identical construction to `ops/buzz/transcript-anchor.py` so a Rust
//! checkpoint signer, the Python ops daemon, and any third-party verifier
//! all produce the same root:
//!
//! - order events by `(created_at, id)`
//! - `leaf_i = SHA256(0x00 || event_id_bytes)`
//! - `node  = SHA256(0x01 || left || right)`; odd node promoted unchanged
//! - `root`  = the single remaining node (1-message window: `root = leaf`)
//!
//! The event id already commits to pubkey/created_at/kind/tags/content —
//! binding the id binds the whole message. `sig` is deliberately NOT part
//! of the committed preimage (relays strip it when serving).

use sha2::{Digest, Sha256};

pub const FORMAT_TAG: &str = "JC-TRANSCRIPT-ANCHOR-v1";

/// `SHA256(0x00 || id_bytes)` — leaf domain separation.
pub fn leaf_hash(event_id: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update([0x00]);
    h.update(event_id);
    h.finalize().into()
}

/// Root over a set of leaf hashes. `leaves` must already be in canonical
/// order (sorted events). Empty input panics — an empty window is not
/// anchored by policy, catch it before calling.
pub fn merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    assert!(!leaves.is_empty(), "empty windows are not anchored");
    let mut level: Vec<[u8; 32]> = leaves.to_vec();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len() / 2 + 1);
        for pair in level.chunks_exact(2) {
            let mut h = Sha256::new();
            h.update([0x01]);
            h.update(pair[0]);
            h.update(pair[1]);
            next.push(h.finalize().into());
        }
        if level.len() % 2 == 1 {
            next.push(level[level.len() - 1]);
        }
        level = next;
    }
    level[0]
}

/// Canonical sort key for the window: `(created_at, id)`.
pub fn event_sort_key(created_at: u64, id_hex: &str) -> (u64, String) {
    (created_at, id_hex.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(b: u8) -> [u8; 32] {
        [b; 32]
    }

    #[test]
    fn single_leaf_is_root() {
        let l = leaf_hash(&id(1));
        assert_eq!(merkle_root(&[l]), l);
    }

    #[test]
    fn odd_node_promoted_not_duplicated() {
        let l: Vec<[u8; 32]> = [id(1), id(2), id(3)].iter().map(leaf_hash).collect();
        // manual: h12 = H(1||l0||l1); root = H(1||h12||l2)
        let mut h = Sha256::new();
        h.update([0x01]);
        h.update(l[0]);
        h.update(l[1]);
        let h12: [u8; 32] = h.finalize().into();
        let mut h = Sha256::new();
        h.update([0x01]);
        h.update(h12);
        h.update(l[2]);
        let want: [u8; 32] = h.finalize().into();
        assert_eq!(merkle_root(&l), want);
    }

    #[test]
    fn matches_python_reference() {
        // Cross-implementation vector: the three event ids below were fed
        // through ops/buzz/transcript-anchor.py (the production ops
        // daemon); the root it produced is the expected value. If this
        // test ever fails, the Rust and Python anchors have diverged.
        let ids = [
            "5ec8a4dc9913ae3d937f13157a433bcc25d5b9a93c1986276ae9f5cca9567b77",
            "d280c6021bf3ee823ce032134fd6200c86af19b6f36d0250fd985b3c4b5f2076",
            "cd068907ee146c8686482f543244ed7d46f27945d7b87ccbacb52833cafec9a5",
        ];
        let leaves: Vec<[u8; 32]> = ids
            .iter()
            .map(|h| leaf_hash(&hex::decode(h).unwrap().try_into().unwrap()))
            .collect();
        let root = merkle_root(&leaves);
        assert_eq!(
            hex::encode(root),
            "957377783a77bfc76f2f7dab687422cd4646d29369e083742a80267a0260a52a"
        );
    }
}
