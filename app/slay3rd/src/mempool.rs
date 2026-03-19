//! Application-managed transaction mempool.
//!
//! Commonware provides no mempool — the application manages its own.
//! Phase 2: simple FIFO queue with capacity limit. No priority, no eviction.

use bytes::Bytes;
use std::collections::VecDeque;

/// Simple FIFO mempool for Phase 2.
///
/// Transactions are submitted via gRPC and drained by the consensus proposer.
/// Thread-safe access is provided by wrapping in `Arc<Mutex<Mempool>>`.
pub struct Mempool {
    queue: VecDeque<Bytes>,
    max_pending: usize,
}

impl Mempool {
    /// Create a new mempool with the given capacity.
    pub fn new(max_pending: usize) -> Self {
        Mempool {
            queue: VecDeque::with_capacity(max_pending.min(1024)),
            max_pending,
        }
    }

    /// Submit a transaction to the mempool.
    /// Returns `true` if accepted, `false` if the mempool is full.
    pub fn submit(&mut self, tx: Bytes) -> bool {
        if self.queue.len() >= self.max_pending {
            return false;
        }
        self.queue.push_back(tx);
        true
    }

    /// Drain up to `max` transactions from the front of the queue.
    /// Returns them in FIFO order. Remaining transactions stay in the queue.
    pub fn drain_batch(&mut self, max: usize) -> Vec<Bytes> {
        let n = max.min(self.queue.len());
        self.queue.drain(..n).collect()
    }

    /// Number of pending transactions.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Whether the mempool is empty.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_submit_and_drain_fifo() {
        let mut pool = Mempool::new(10);
        let tx1 = Bytes::from("tx1");
        let tx2 = Bytes::from("tx2");
        let tx3 = Bytes::from("tx3");

        assert!(pool.submit(tx1.clone()));
        assert!(pool.submit(tx2.clone()));
        assert!(pool.submit(tx3.clone()));

        let batch = pool.drain_batch(3);
        assert_eq!(batch.len(), 3);
        assert_eq!(batch[0], tx1, "FIFO order violated: first tx should be tx1");
        assert_eq!(batch[1], tx2, "FIFO order violated: second tx should be tx2");
        assert_eq!(batch[2], tx3, "FIFO order violated: third tx should be tx3");
        assert!(pool.is_empty());
    }

    #[test]
    fn test_submit_full_mempool() {
        let mut pool = Mempool::new(2);
        assert!(pool.submit(Bytes::from("tx1")));
        assert!(pool.submit(Bytes::from("tx2")));
        // Pool is now full
        assert!(!pool.submit(Bytes::from("tx3")), "submit should return false when pool is full");
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn test_drain_partial() {
        let mut pool = Mempool::new(10);
        for i in 0..5 {
            pool.submit(Bytes::from(format!("tx{}", i)));
        }
        let batch = pool.drain_batch(2);
        assert_eq!(batch.len(), 2);
        assert_eq!(pool.len(), 3, "3 transactions should remain after draining 2 from 5");
    }

    #[test]
    fn test_drain_empty() {
        let mut pool = Mempool::new(10);
        let batch = pool.drain_batch(5);
        assert!(batch.is_empty(), "drain on empty mempool should return empty vec");
    }

    #[test]
    fn test_drain_more_than_available() {
        let mut pool = Mempool::new(10);
        pool.submit(Bytes::from("tx1"));
        pool.submit(Bytes::from("tx2"));
        let batch = pool.drain_batch(100);
        assert_eq!(batch.len(), 2, "should drain only available txs");
        assert!(pool.is_empty());
    }
}
