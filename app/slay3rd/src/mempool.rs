//! Application-managed transaction mempool.
//!
//! Commonware provides no mempool — the application manages its own. This is
//! node-local policy, NOT consensus state:
//! - FIFO by arrival, deduplicated by sha256(tx bytes) (the Cosmos txhash).
//! - Proposals only PEEK. A tx leaves the pool when a block containing it is
//!   executed (`remove_committed`) or when the post-block recheck finds it
//!   invalid against the new state (`remove`). A proposal that consensus
//!   later skips therefore loses no transactions.
//! - Bounded by tx count, total bytes, and per-tx size.

use bytes::Bytes;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};

/// Largest single tx accepted (fits a store-code tx with a large contract).
pub const DEFAULT_MAX_TX_BYTES: usize = 2 * 1024 * 1024;
/// Total bytes the pool may hold.
pub const DEFAULT_MAX_POOL_BYTES: usize = 64 * 1024 * 1024;

pub type TxHash = [u8; 32];

/// sha256 of the raw tx bytes (same hash as the Cosmos `txhash`).
pub fn tx_hash(raw: &[u8]) -> TxHash {
    Sha256::digest(raw).into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitError {
    /// An identical tx is already pending.
    Duplicate,
    /// The tx exceeds the per-tx size limit.
    TooLarge,
    /// The pool is at its count or byte capacity.
    Full,
}

/// FIFO, hash-deduplicated mempool. Wrap in `Arc<Mutex<Mempool>>` for sharing.
pub struct Mempool {
    /// Arrival sequence -> (hash, bytes). Iteration order is FIFO.
    order: BTreeMap<u64, (TxHash, Bytes)>,
    /// Hash -> arrival sequence.
    index: HashMap<TxHash, u64>,
    next_seq: u64,
    total_bytes: usize,
    max_pending: usize,
    max_total_bytes: usize,
    max_tx_bytes: usize,
}

impl Mempool {
    /// Create a mempool holding at most `max_pending` txs (default byte limits).
    pub fn new(max_pending: usize) -> Self {
        Self::with_limits(max_pending, DEFAULT_MAX_POOL_BYTES, DEFAULT_MAX_TX_BYTES)
    }

    pub fn with_limits(max_pending: usize, max_total_bytes: usize, max_tx_bytes: usize) -> Self {
        Mempool {
            order: BTreeMap::new(),
            index: HashMap::new(),
            next_seq: 0,
            total_bytes: 0,
            max_pending,
            max_total_bytes,
            max_tx_bytes,
        }
    }

    /// Add a tx. Returns its hash on success.
    pub fn submit(&mut self, tx: Bytes) -> Result<TxHash, SubmitError> {
        if tx.len() > self.max_tx_bytes {
            return Err(SubmitError::TooLarge);
        }
        let hash = tx_hash(&tx);
        if self.index.contains_key(&hash) {
            return Err(SubmitError::Duplicate);
        }
        if self.order.len() >= self.max_pending
            || self.total_bytes.saturating_add(tx.len()) > self.max_total_bytes
        {
            return Err(SubmitError::Full);
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        self.total_bytes += tx.len();
        self.index.insert(hash, seq);
        self.order.insert(seq, (hash, tx));
        Ok(hash)
    }

    /// Whether a tx with this hash is pending.
    pub fn contains(&self, hash: &TxHash) -> bool {
        self.index.contains_key(hash)
    }

    /// Select up to `max_txs` txs totalling at most `max_bytes`, in FIFO
    /// order, WITHOUT removing them. A tx that does not fit the remaining
    /// byte budget is skipped so it cannot block smaller txs behind it.
    pub fn peek_batch(&self, max_txs: usize, max_bytes: usize) -> Vec<Bytes> {
        let mut out = Vec::new();
        let mut bytes = 0usize;
        for (_, tx) in self.order.values() {
            if out.len() >= max_txs {
                break;
            }
            if bytes + tx.len() > max_bytes {
                continue;
            }
            bytes += tx.len();
            out.push(tx.clone());
        }
        out
    }

    /// Remove one tx by hash. Returns whether it was present.
    pub fn remove(&mut self, hash: &TxHash) -> bool {
        match self.index.remove(hash) {
            Some(seq) => {
                if let Some((_, tx)) = self.order.remove(&seq) {
                    self.total_bytes -= tx.len();
                }
                true
            }
            None => false,
        }
    }

    /// Remove every tx included in an executed block. Returns how many were pending.
    pub fn remove_committed<'a>(&mut self, hashes: impl IntoIterator<Item = &'a TxHash>) -> usize {
        hashes.into_iter().filter(|h| self.remove(h)).count()
    }

    /// All pending txs in FIFO order (for post-block recheck).
    pub fn snapshot(&self) -> Vec<(TxHash, Bytes)> {
        self.order.values().cloned().collect()
    }

    /// Number of pending transactions.
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Whether the mempool is empty.
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Total bytes of pending transactions.
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BIG: usize = usize::MAX;

    #[test]
    fn test_submit_and_peek_fifo() {
        let mut pool = Mempool::new(10);
        let tx1 = Bytes::from("tx1");
        let tx2 = Bytes::from("tx2");
        let tx3 = Bytes::from("tx3");

        assert!(pool.submit(tx1.clone()).is_ok());
        assert!(pool.submit(tx2.clone()).is_ok());
        assert!(pool.submit(tx3.clone()).is_ok());

        let batch = pool.peek_batch(3, BIG);
        assert_eq!(batch.len(), 3);
        assert_eq!(batch[0], tx1, "FIFO order violated: first tx should be tx1");
        assert_eq!(batch[1], tx2, "FIFO order violated: second tx should be tx2");
        assert_eq!(batch[2], tx3, "FIFO order violated: third tx should be tx3");
    }

    #[test]
    fn test_submit_full_mempool() {
        let mut pool = Mempool::new(2);
        assert!(pool.submit(Bytes::from("tx1")).is_ok());
        assert!(pool.submit(Bytes::from("tx2")).is_ok());
        // Pool is now full
        assert_eq!(pool.submit(Bytes::from("tx3")), Err(SubmitError::Full));
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn test_peek_partial() {
        let mut pool = Mempool::new(10);
        for i in 0..5 {
            pool.submit(Bytes::from(format!("tx{}", i))).unwrap();
        }
        let batch = pool.peek_batch(2, BIG);
        assert_eq!(batch.len(), 2);
        assert_eq!(pool.len(), 5, "peek must not remove transactions");
    }

    #[test]
    fn test_peek_empty() {
        let pool = Mempool::new(10);
        assert!(pool.peek_batch(5, BIG).is_empty(), "peek on empty mempool should return empty vec");
    }

    #[test]
    fn test_peek_more_than_available() {
        let mut pool = Mempool::new(10);
        pool.submit(Bytes::from("tx1")).unwrap();
        pool.submit(Bytes::from("tx2")).unwrap();
        assert_eq!(pool.peek_batch(100, BIG).len(), 2, "should return only available txs");
    }

    #[test]
    fn test_duplicate_rejected() {
        let mut pool = Mempool::new(10);
        let h = pool.submit(Bytes::from("tx1")).unwrap();
        assert_eq!(h, tx_hash(b"tx1"));
        assert_eq!(pool.submit(Bytes::from("tx1")), Err(SubmitError::Duplicate));
        assert_eq!(pool.len(), 1);
        assert!(pool.contains(&h));
    }

    /// A tx survives an unfinalized proposal and leaves only once committed.
    #[test]
    fn test_remove_committed_only() {
        let mut pool = Mempool::new(10);
        let h1 = pool.submit(Bytes::from("tx1")).unwrap();
        let h2 = pool.submit(Bytes::from("tx2")).unwrap();
        // proposal includes both; consensus skips it -> nothing is lost
        assert_eq!(pool.peek_batch(10, BIG).len(), 2);
        assert_eq!(pool.len(), 2);
        // a later block commits tx1 (plus a tx this node never saw)
        let foreign = tx_hash(b"other");
        assert_eq!(pool.remove_committed([&h1, &foreign]), 1);
        assert_eq!(pool.peek_batch(10, BIG), vec![Bytes::from("tx2")]);
        assert!(!pool.contains(&h1));
        assert!(pool.contains(&h2));
        assert_eq!(pool.total_bytes(), 3);
        // resubmitting a committed tx is accepted by the pool (check_tx rejects it upstream)
        assert!(pool.submit(Bytes::from("tx1")).is_ok());
    }

    #[test]
    fn test_byte_limits() {
        let mut pool = Mempool::with_limits(10, 10, 4);
        assert_eq!(pool.submit(Bytes::from("12345")), Err(SubmitError::TooLarge));
        pool.submit(Bytes::from("aaaa")).unwrap();
        pool.submit(Bytes::from("bbbb")).unwrap();
        assert_eq!(pool.submit(Bytes::from("ccc")), Err(SubmitError::Full));
        pool.submit(Bytes::from("cc")).unwrap();
        assert_eq!(pool.total_bytes(), 10);
        assert!(pool.remove(&tx_hash(b"aaaa")));
        assert_eq!(pool.total_bytes(), 6);
    }

    #[test]
    fn test_peek_skips_tx_exceeding_byte_budget() {
        let mut pool = Mempool::new(10);
        pool.submit(Bytes::from("aaaaaa")).unwrap();
        pool.submit(Bytes::from("bb")).unwrap();
        pool.submit(Bytes::from("cc")).unwrap();
        assert_eq!(
            pool.peek_batch(10, 5),
            vec![Bytes::from("bb"), Bytes::from("cc")],
            "an oversized head tx must not block smaller txs"
        );
    }
}
