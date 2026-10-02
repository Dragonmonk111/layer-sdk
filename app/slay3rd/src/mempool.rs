//! Application-managed transaction mempool.
//!
//! Commonware provides no mempool — the application manages its own. This is
//! node-local policy, NOT consensus state:
//! - FIFO by arrival, deduplicated by sha256(tx bytes) (the Cosmos txhash).
//! - Proposals only PEEK. A tx leaves the pool when a block containing it is
//!   executed (`remove_committed`) or when the post-block recheck finds it
//!   invalid against the new state (`remove`). A proposal that consensus
//!   later skips therefore loses no transactions.
//! - Bounded by tx count, total bytes, per-tx size, and per-sender count (M7).
//! - Proposal ordering (M4): FIFO fairness across senders, but each sender's
//!   txs are emitted contiguously in ascending sequence order so a block
//!   never places seq n+1 ahead of seq n for the same account.

use bytes::Bytes;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};

/// Largest single tx accepted.
pub const DEFAULT_MAX_TX_BYTES: usize = 2 * 1024 * 1024;
/// Larger cap for txs carrying a `MsgStoreCode` wasm blob (M7 exception).
pub const MAX_STORE_CODE_TX_BYTES: usize = 8 * 1024 * 1024;
/// Total bytes the pool may hold.
pub const DEFAULT_MAX_POOL_BYTES: usize = 64 * 1024 * 1024;
/// Pending txs per sender (M7).
pub const DEFAULT_MAX_PER_SENDER: usize = 64;

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
    /// The sender is at its per-sender pending cap.
    SenderFull,
}

/// Optional admission metadata recorded alongside a tx. Callers that have
/// already decoded the tx (BroadcastTx, gossip) pass it; enables per-sender
/// ordering (M4) and caps (M7). Raw submissions may omit it — they keep
/// pure FIFO behavior.
#[derive(Debug, Clone)]
pub struct TxMeta {
    /// Bech32 signer address — the per-sender bucket key.
    pub sender: String,
    /// Account sequence — used to order this sender's txs.
    pub sequence: u64,
    /// Tx carries a `MsgStoreCode` — exempt from `max_tx_bytes`, still
    /// bounded by `MAX_STORE_CODE_TX_BYTES` and the pool byte budget.
    pub store_code: bool,
}

/// FIFO, hash-deduplicated mempool. Wrap in `Arc<Mutex<Mempool>>` for sharing.
pub struct Mempool {
    /// Arrival sequence -> (hash, bytes). Iteration order is FIFO.
    order: BTreeMap<u64, (TxHash, Bytes)>,
    /// Hash -> arrival sequence.
    index: HashMap<TxHash, u64>,
    /// Hash -> sender/sequence metadata (when supplied at submit).
    meta: HashMap<TxHash, TxMeta>,
    /// Sender -> pending count (M7 cap enforcement).
    sender_counts: HashMap<String, usize>,
    next_seq: u64,
    total_bytes: usize,
    max_pending: usize,
    max_total_bytes: usize,
    max_tx_bytes: usize,
    max_per_sender: usize,
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
            meta: HashMap::new(),
            sender_counts: HashMap::new(),
            next_seq: 0,
            total_bytes: 0,
            max_pending,
            max_total_bytes,
            max_tx_bytes,
            max_per_sender: DEFAULT_MAX_PER_SENDER,
        }
    }

    /// Builder: override the per-sender cap (tests / config).
    pub fn with_sender_cap(mut self, max_per_sender: usize) -> Self {
        self.max_per_sender = max_per_sender;
        self
    }

    /// Add a tx. Returns its hash on success.
    pub fn submit(&mut self, tx: Bytes) -> Result<TxHash, SubmitError> {
        self.submit_checked(tx, None)
    }

    /// Add a tx with sender metadata (M4 ordering + M7 caps).
    pub fn submit_checked(
        &mut self,
        tx: Bytes,
        meta: Option<TxMeta>,
    ) -> Result<TxHash, SubmitError> {
        let tx_cap = match &meta {
            Some(m) if m.store_code => MAX_STORE_CODE_TX_BYTES,
            _ => self.max_tx_bytes,
        };
        if tx.len() > tx_cap {
            return Err(SubmitError::TooLarge);
        }
        let hash = tx_hash(&tx);
        if self.index.contains_key(&hash) {
            return Err(SubmitError::Duplicate);
        }
        if let Some(m) = &meta {
            if self.sender_counts.get(&m.sender).copied().unwrap_or(0)
                >= self.max_per_sender
            {
                return Err(SubmitError::SenderFull);
            }
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
        if let Some(m) = meta {
            *self.sender_counts.entry(m.sender.clone()).or_insert(0) += 1;
            self.meta.insert(hash, m);
        }
        Ok(hash)
    }

    /// Whether a tx with this hash is pending.
    pub fn contains(&self, hash: &TxHash) -> bool {
        self.index.contains_key(hash)
    }

    /// Select up to `max_txs` txs totalling at most `max_bytes`, WITHOUT
    /// removing them.
    ///
    /// Ordering (M4): FIFO fairness across senders, but each sender's txs
    /// are emitted contiguously in ascending sequence order so a proposed
    /// block never places seq n+1 ahead of seq n for the same account.
    /// Txs submitted without metadata keep pure FIFO. A tx that does not
    /// fit the remaining byte budget is skipped so it cannot block smaller
    /// txs behind it.
    pub fn peek_batch(&self, max_txs: usize, max_bytes: usize) -> Vec<Bytes> {
        use std::collections::{BTreeSet, VecDeque};

        // One queue per sender holding (sequence, arrival) sorted by
        // sequence. Meta-less txs get a unique key — a singleton FIFO queue.
        let mut queues: HashMap<String, VecDeque<(u64, u64)>> = HashMap::new();
        for (arrival, (hash, _)) in &self.order {
            let (key, seq) = match self.meta.get(hash) {
                Some(m) => (m.sender.clone(), m.sequence),
                None => (hex::encode(hash), u64::MAX),
            };
            queues.entry(key).or_default().push_back((seq, *arrival));
        }
        for q in queues.values_mut() {
            let mut sorted: Vec<(u64, u64)> = q.iter().copied().collect();
            sorted.sort_unstable();
            *q = sorted.into_iter().collect();
        }

        // Merge by queue-head arrival: the head with the smallest arrival
        // wins; consuming it promotes that sender's next-lowest sequence.
        let mut heads: BTreeSet<(u64, String)> = queues
            .iter()
            .map(|(k, q)| (q.front().unwrap().1, k.clone()))
            .collect();
        let mut out = Vec::new();
        let mut bytes = 0usize;
        while out.len() < max_txs {
            let Some((head_arrival, key)) = heads.iter().next().cloned() else {
                break;
            };
            heads.remove(&(head_arrival, key.clone()));
            let queue = queues.get_mut(&key).expect("queue exists for head");
            let (_, arrival) = queue.pop_front().unwrap();
            if let Some((_, next_arrival)) = queue.front() {
                heads.insert((*next_arrival, key));
            }
            let tx = &self.order[&arrival].1;
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
                if let Some(m) = self.meta.remove(hash) {
                    if let Some(c) = self.sender_counts.get_mut(&m.sender) {
                        *c = c.saturating_sub(1);
                        if *c == 0 {
                            self.sender_counts.remove(&m.sender);
                        }
                    }
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

    fn meta(sender: &str, sequence: u64) -> TxMeta {
        TxMeta {
            sender: sender.to_string(),
            sequence,
            store_code: false,
        }
    }

    /// M4: a sender's txs are emitted contiguously in ascending sequence
    /// order regardless of arrival order; FIFO fairness is preserved
    /// across senders via each queue head's arrival time.
    #[test]
    fn test_sender_sequence_ordering() {
        let mut pool = Mempool::new(10);
        // sender A's txs arrive out of order (seq 2 before seq 1)
        pool.submit_checked(Bytes::from("a2"), Some(meta("alice", 2)))
            .unwrap();
        pool.submit_checked(Bytes::from("a1"), Some(meta("alice", 1)))
            .unwrap();
        pool.submit_checked(Bytes::from("b0"), Some(meta("bob", 0)))
            .unwrap();
        assert_eq!(
            pool.peek_batch(10, BIG),
            vec![Bytes::from("a1"), Bytes::from("a2"), Bytes::from("b0")],
            "alice's txs must be contiguous and sequence-ordered"
        );
    }

    /// M4: a meta-less tx keeps FIFO behavior alongside meta'd senders.
    #[test]
    fn test_metaless_txs_keep_fifo() {
        let mut pool = Mempool::new(10);
        pool.submit(Bytes::from("raw")).unwrap();
        pool.submit_checked(Bytes::from("a5"), Some(meta("alice", 5)))
            .unwrap();
        assert_eq!(
            pool.peek_batch(10, BIG),
            vec![Bytes::from("raw"), Bytes::from("a5")]
        );
    }

    /// M7: per-sender cap rejects the N+1th pending tx from one sender
    /// while other senders are unaffected; removal frees the slot.
    #[test]
    fn test_per_sender_cap() {
        let mut pool = Mempool::new(10).with_sender_cap(2);
        pool.submit_checked(Bytes::from("a1"), Some(meta("alice", 1)))
            .unwrap();
        pool.submit_checked(Bytes::from("a2"), Some(meta("alice", 2)))
            .unwrap();
        assert_eq!(
            pool.submit_checked(Bytes::from("a3"), Some(meta("alice", 3))),
            Err(SubmitError::SenderFull)
        );
        // a different sender is unaffected
        pool.submit_checked(Bytes::from("b1"), Some(meta("bob", 1)))
            .unwrap();
        // removal frees alice's slot
        pool.remove(&tx_hash(b"a1"));
        assert!(pool
            .submit_checked(Bytes::from("a3"), Some(meta("alice", 3)))
            .is_ok());
    }

    /// M7: a store-code tx may exceed max_tx_bytes (bounded by
    /// MAX_STORE_CODE_TX_BYTES); normal txs are still capped.
    #[test]
    fn test_store_code_size_exception() {
        let mut pool = Mempool::with_limits(10, usize::MAX, 4);
        assert_eq!(
            pool.submit(Bytes::from(vec![0u8; 5])),
            Err(SubmitError::TooLarge)
        );
        let m = TxMeta {
            sender: "alice".to_string(),
            sequence: 0,
            store_code: true,
        };
        assert!(pool.submit_checked(Bytes::from(vec![0u8; 5]), Some(m)).is_ok());
    }

    /// Sender counters must be released when a tx leaves the pool.
    #[test]
    fn test_meta_released_on_remove() {
        let mut pool = Mempool::new(10).with_sender_cap(1);
        pool.submit_checked(Bytes::from("a1"), Some(meta("alice", 1)))
            .unwrap();
        pool.remove(&tx_hash(b"a1"));
        pool.submit_checked(Bytes::from("a2"), Some(meta("alice", 2)))
            .unwrap();
    }
}
