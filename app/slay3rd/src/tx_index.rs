//! Node-local transaction result index backing `cosmos.tx.v1beta1.Service/GetTx`.
//!
//! This is NOT application state: it is written after `finalize_block` commits,
//! never read during execution, and never hashed into `app_hash`. Each node may
//! hold a different window of history without affecting consensus.
//!
//! Bounded FIFO: once `capacity` entries are held, the oldest is evicted.
//! The in-memory map is a hot cache — `execute_block` also writes every
//! result through to durable storage under `_txres/`, and `get_tx` falls
//! back to it on a cache miss (Q2).

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use layer_proto::cosmos::base::abci::v1beta1::TxResponse;
use layer_proto::tendermint::abci::{Event, EventAttribute};
use layer_std::api::TxResult;

/// Default number of tx results retained per node.
pub const DEFAULT_TX_INDEX_CAPACITY: usize = 100_000;

/// Codespace reported for failed txs (matches the convention in `layer_std::api::tx`).
const ERROR_CODESPACE: &str = "slay3r";

struct Inner {
    map: HashMap<String, TxResponse>,
    order: VecDeque<String>,
}

pub struct TxIndex {
    inner: Mutex<Inner>,
    capacity: usize,
}

impl TxIndex {
    pub fn new(capacity: usize) -> Self {
        TxIndex {
            inner: Mutex::new(Inner {
                map: HashMap::new(),
                order: VecDeque::new(),
            }),
            capacity: capacity.max(1),
        }
    }

    /// Insert a result keyed by its (upper-case hex) txhash.
    pub fn insert(&self, resp: TxResponse) {
        let key = normalize(&resp.txhash);
        let mut g = self.inner.lock().expect("tx index lock poisoned");
        if g.map.insert(key.clone(), resp).is_none() {
            g.order.push_back(key);
            while g.order.len() > self.capacity {
                if let Some(old) = g.order.pop_front() {
                    g.map.remove(&old);
                }
            }
        }
    }

    /// Look up a result by txhash (case-insensitive, optional `0x` prefix).
    pub fn get(&self, hash: &str) -> Option<TxResponse> {
        let g = self.inner.lock().expect("tx index lock poisoned");
        g.map.get(&normalize(hash)).cloned()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().expect("tx index lock poisoned").map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Normalize a txhash for indexing: trim, strip an optional `0x` prefix,
/// upper-case. Used for both the in-memory cache and the `_txres/` store keys.
pub fn normalize(hash: &str) -> String {
    let h = hash.trim();
    let h = h.strip_prefix("0x").or_else(|| h.strip_prefix("0X")).unwrap_or(h);
    h.to_ascii_uppercase()
}

/// Upper-case hex sha256 of raw tx bytes (CometBFT txhash convention).
pub fn tx_hash_hex(raw: &[u8]) -> String {
    hex::encode_upper(<sha2::Sha256 as sha2::Digest>::digest(raw))
}

/// Build a Cosmos `TxResponse` from an executed tx result.
pub fn tx_response_from_result<E: std::error::Error>(
    txhash: String,
    height: u64,
    res: &TxResult<E>,
) -> TxResponse {
    let (code, codespace, raw_log, events) = match &res.result {
        Ok(ok) => (
            0,
            String::new(),
            String::new(),
            ok.events
                .iter()
                .flatten()
                .map(|e| Event {
                    r#type: e.ty.clone(),
                    attributes: e
                        .attributes
                        .iter()
                        .map(|a| EventAttribute {
                            key: a.key.clone(),
                            value: a.value.clone(),
                            index: false,
                        })
                        .collect(),
                })
                .collect(),
        ),
        Err(e) => (1, ERROR_CODESPACE.to_string(), e.to_string(), vec![]),
    };
    TxResponse {
        height: height as i64,
        txhash,
        codespace,
        code,
        raw_log,
        gas_wanted: res.gas.gas_wanted as i64,
        gas_used: res.gas.gas_used as i64,
        events,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::Event as CwEvent;
    use layer_std::api::{GasInfo, TxResponse as LayerTxResponse};

    #[derive(Debug)]
    struct TestErr(&'static str);
    impl std::fmt::Display for TestErr {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.0)
        }
    }
    impl std::error::Error for TestErr {}

    fn resp(hash: &str) -> TxResponse {
        TxResponse {
            txhash: hash.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn get_is_case_insensitive_and_strips_0x() {
        let idx = TxIndex::new(10);
        idx.insert(resp("ABCDEF"));
        assert!(idx.get("abcdef").is_some());
        assert!(idx.get("0xAbCdEf").is_some());
        assert!(idx.get("  ABCDEF ").is_some());
        assert!(idx.get("123456").is_none());
    }

    #[test]
    fn evicts_oldest_beyond_capacity() {
        let idx = TxIndex::new(2);
        idx.insert(resp("AA"));
        idx.insert(resp("BB"));
        idx.insert(resp("CC"));
        assert_eq!(idx.len(), 2);
        assert!(idx.get("AA").is_none());
        assert!(idx.get("BB").is_some());
        assert!(idx.get("CC").is_some());
    }

    #[test]
    fn reinsert_same_hash_does_not_grow_order() {
        let idx = TxIndex::new(2);
        idx.insert(resp("AA"));
        idx.insert(resp("AA"));
        idx.insert(resp("BB"));
        assert_eq!(idx.len(), 2);
        assert!(idx.get("AA").is_some());
    }

    #[test]
    fn tx_hash_matches_sha256_upper_hex() {
        assert_eq!(
            tx_hash_hex(b""),
            "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855"
        );
    }

    #[test]
    fn success_result_maps_events_and_gas() {
        let res: TxResult<TestErr> = TxResult {
            gas: GasInfo { gas_used: 725_804, gas_wanted: 4_000_000 },
            result: Ok(LayerTxResponse::new(
                vec![],
                vec![vec![CwEvent::new("wasm").add_attribute("valid", "true")]],
            )),
        };
        let r = tx_response_from_result("AB".into(), 7, &res);
        assert_eq!(r.code, 0);
        assert_eq!(r.height, 7);
        assert_eq!(r.gas_used, 725_804);
        assert_eq!(r.gas_wanted, 4_000_000);
        assert_eq!(r.events.len(), 1);
        assert_eq!(r.events[0].r#type, "wasm");
        assert_eq!(r.events[0].attributes[0].key, "valid");
        assert_eq!(r.events[0].attributes[0].value, "true");
    }

    #[test]
    fn failed_result_maps_code_and_log() {
        let res: TxResult<TestErr> = TxResult {
            gas: GasInfo { gas_used: 309_573, gas_wanted: 4_000_000 },
            result: Err(TestErr("MAYO signature verification failed")),
        };
        let r = tx_response_from_result("CD".into(), 9, &res);
        assert_eq!(r.code, 1);
        assert_eq!(r.codespace, "slay3r");
        assert_eq!(r.raw_log, "MAYO signature verification failed");
        assert!(r.events.is_empty());
    }
}
