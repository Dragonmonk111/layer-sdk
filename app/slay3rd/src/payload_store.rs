//! Durable, digest-keyed store for block payloads.
//!
//! Consensus only certifies 32-byte digests; the payload bytes behind a
//! digest must outlive process restarts, otherwise a payload that was
//! notarized (or finalized) but not yet executed is unrecoverable and the
//! chain wedges. Each payload is written to `{dir}/{hex(digest)}.{height}`
//! when this node certifies it (or executes it). Executed payloads are kept
//! for a retention window so replayed finalizations can be recognised as
//! stale and so peers that missed the relay can fetch them.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::block::BlockPayload;

/// Default number of executed heights retained on disk.
///
/// Backfill beyond this window is served from the app's `_payload/`
/// sidecar, which the node's pruning tier retains (validator: 540,000
/// heights); past that, a node needs state sync. 64k payload files cost
/// ~270 MB on disk (one 4 KiB block each).
pub const DEFAULT_RETAIN_HEIGHTS: u64 = 65_536;

pub struct PayloadStore {
    /// `None` = in-memory mode (tests): nothing is persisted.
    dir: Option<PathBuf>,
    /// digest -> payload height, for every payload present on disk.
    index: BTreeMap<[u8; 32], u64>,
    /// height -> digest reverse index, enabling peers to serve payload
    /// backfill requests by height without knowing digests.
    by_height: BTreeMap<u64, [u8; 32]>,
}

impl PayloadStore {
    /// A store that persists nothing (unit tests / ephemeral nodes).
    pub fn in_memory() -> Self {
        PayloadStore {
            dir: None,
            index: BTreeMap::new(),
            by_height: BTreeMap::new(),
        }
    }

    /// Open (or create) a store rooted at `dir`, indexing existing files.
    pub fn open(dir: impl Into<PathBuf>) -> std::io::Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        let mut index = BTreeMap::new();
        let mut by_height = BTreeMap::new();
        for entry in std::fs::read_dir(&dir)? {
            let name = entry?.file_name();
            let Some(name) = name.to_str() else { continue };
            if let Some((digest, height)) = parse_file_name(name) {
                index.insert(digest, height);
                by_height.insert(height, digest);
            }
        }
        Ok(PayloadStore {
            dir: Some(dir),
            index,
            by_height,
        })
    }

    fn path(&self, digest: &[u8; 32], height: u64) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join(format!("{}.{}", hex::encode(digest), height)))
    }

    pub fn contains(&self, digest: &[u8; 32]) -> bool {
        self.index.contains_key(digest)
    }

    pub fn get(&self, digest: &[u8; 32]) -> Option<BlockPayload> {
        let height = *self.index.get(digest)?;
        let bytes = std::fs::read(self.path(digest, height)?).ok()?;
        let payload = BlockPayload::from_bytes(&bytes).ok()?;
        (payload.digest() == *digest).then_some(payload)
    }

    /// Fetch a stored payload by block height (peer backfill path).
    pub fn get_by_height(&self, height: u64) -> Option<BlockPayload> {
        let digest = *self.by_height.get(&height)?;
        self.get(&digest)
    }

    pub fn contains_height(&self, height: u64) -> bool {
        self.by_height.contains_key(&height)
    }

    /// Persist a payload. Write-then-rename so a crash never leaves a torn file.
    pub fn put(&mut self, payload: &BlockPayload) -> std::io::Result<()> {
        let digest = payload.digest();
        if self.index.contains_key(&digest) {
            return Ok(());
        }
        let Some(path) = self.path(&digest, payload.height) else {
            return Ok(());
        };
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, payload.to_bytes())?;
        std::fs::rename(&tmp, &path)?;
        self.index.insert(digest, payload.height);
        self.by_height.insert(payload.height, digest);
        Ok(())
    }

    /// Delete every payload whose height is below `height`.
    pub fn prune_below(&mut self, height: u64) {
        let stale: Vec<([u8; 32], u64)> = self
            .index
            .iter()
            .filter(|(_, h)| **h < height)
            .map(|(d, h)| (*d, *h))
            .collect();
        for (digest, h) in stale {
            if let Some(path) = self.path(&digest, h) {
                let _ = std::fs::remove_file(path);
            }
            self.index.remove(&digest);
            self.by_height.remove(&h);
        }
    }

    /// All stored payloads with height strictly above `height`
    /// (i.e. certified but not yet executed at startup).
    pub fn payloads_above(&self, height: u64) -> Vec<BlockPayload> {
        self.index
            .iter()
            .filter(|(_, h)| **h > height)
            .filter_map(|(d, _)| self.get(d))
            .collect()
    }
}

fn parse_file_name(name: &str) -> Option<([u8; 32], u64)> {
    let (hex_digest, height) = name.split_once('.')?;
    let height: u64 = height.parse().ok()?;
    let digest: [u8; 32] = hex::decode(hex_digest).ok()?.try_into().ok()?;
    Some((digest, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(height: u64) -> BlockPayload {
        BlockPayload {
            height,
            timestamp_nanos: height,
            proposer: vec![1; 32],
            txs: vec![],
            parent_digest: [0; 32],
            state_root: [0; 32],
        }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("slay3rd-payload-store-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn put_get_survives_reopen() {
        let dir = temp_dir("reopen");
        let p = payload(7);
        {
            let mut s = PayloadStore::open(&dir).unwrap();
            s.put(&p).unwrap();
        }
        let s = PayloadStore::open(&dir).unwrap();
        assert_eq!(s.get(&p.digest()), Some(p.clone()));
        assert_eq!(s.payloads_above(6), vec![p.clone()]);
        assert!(s.payloads_above(7).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn get_by_height_roundtrip() {
        let dir = temp_dir("by-height");
        let (a, b) = (payload(3), payload(4));
        {
            let mut s = PayloadStore::open(&dir).unwrap();
            s.put(&a).unwrap();
            s.put(&b).unwrap();
        }
        let s = PayloadStore::open(&dir).unwrap();
        assert_eq!(s.get_by_height(3), Some(a.clone()));
        assert_eq!(s.get_by_height(4), Some(b.clone()));
        assert!(s.get_by_height(5).is_none());
        assert!(s.contains_height(3));
        assert!(!s.contains_height(9));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_below_removes_old_heights() {
        let dir = temp_dir("prune");
        let mut s = PayloadStore::open(&dir).unwrap();
        let (a, b) = (payload(1), payload(5));
        s.put(&a).unwrap();
        s.put(&b).unwrap();
        s.prune_below(5);
        assert!(!s.contains(&a.digest()));
        assert!(s.contains(&b.digest()));
        let s2 = PayloadStore::open(&dir).unwrap();
        assert!(!s2.contains(&a.digest()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn in_memory_persists_nothing() {
        let mut s = PayloadStore::in_memory();
        let p = payload(1);
        s.put(&p).unwrap();
        assert!(s.get(&p.digest()).is_none());
    }
}
