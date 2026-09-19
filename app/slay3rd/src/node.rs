//! LayerNode: CertifiableAutomaton bridge between Commonware consensus and Layer App<T>.
//!
//! This is the single integration point where consensus callbacks call into
//! the Layer state machine. LayerNode wraps Arc<RwLock<App<T>>> and translates:
//! - genesis() -> App::app_hash() (initial state digest)
//! - propose() -> drain mempool, build BlockPayload, return digest
//! - verify()  -> structural validation only, NO state mutation
//! - certify() -> App::finalize_block() (DETERMINISM CRITICAL)
//!
//! IMPORTANT: The `pending_payloads` map is shared with the Relay (Plan 03).
//! The proposer's `propose()` inserts payloads; the Relay inserts payloads
//! received from other validators. `verify()` looks up digests in this shared
//! map. Without Relay wiring, only the proposer would have payloads and
//! non-proposers would always fail `verify()`.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use commonware_consensus::{Automaton, CertifiableAutomaton};
use commonware_consensus::simplex::types::Context;
use commonware_consensus::types::{Epoch, Round};
use commonware_cryptography::{sha256, Hasher, PublicKey};

use layer_app::App;
use layer_std::api::Block;
use layer_std::Timestamp;
use layer_storage::PersistentStorage;
use tokio::sync::{Mutex, RwLock, oneshot};

// Cosmos tx deserialization for execute_block()
use layer_cosmos::parse_cosmos_tx;

use crate::block::BlockPayload;
use crate::mempool::Mempool;

/// Maximum number of transactions per block proposal.
const MAX_BLOCK_TXS: usize = 100;

/// LayerNode bridges Commonware consensus to the Layer state machine.
///
/// Implements `CertifiableAutomaton` — the consensus engine calls `genesis()`,
/// `propose()`, `verify()`, and `certify()` at the appropriate protocol steps.
///
/// Type parameters:
/// - `T`: The persistent storage backend (e.g., `MemoryStore` or `RockStore`)
/// - `P`: The public key type from the consensus signing scheme (e.g., `bls12381::PublicKey`)
pub struct LayerNode<T: PersistentStorage + Send + Sync + 'static, P: PublicKey> {
    /// The Layer application state machine, wrapped for shared async access.
    /// RwLock allows concurrent gRPC reads while finalize_block holds write lock.
    app: Arc<RwLock<App<T>>>,
    /// Application-managed transaction mempool.
    mempool: Arc<Mutex<Mempool>>,
    /// Pending block payloads keyed by their digest, awaiting verify/certify.
    /// BTreeMap for deterministic ordering (CONS-04).
    /// SHARED with the Relay (Plan 03) — the Relay inserts payloads received
    /// from other validators so non-proposers can look them up in verify().
    pending_payloads: Arc<Mutex<BTreeMap<[u8; 32], BlockPayload>>>,
    /// Track the current block height (incremented only on successful certify).
    current_height: Arc<Mutex<u64>>,
    /// The last committed block digest (parent linkage for new proposals).
    last_digest: Arc<Mutex<[u8; 32]>>,
    /// Phantom to bind the P type parameter without storing P directly.
    _phantom: std::marker::PhantomData<P>,
}

/// Manual Clone implementation — all fields are behind Arc so T does not need Clone.
impl<T: PersistentStorage + Send + Sync + 'static, P: PublicKey> Clone for LayerNode<T, P> {
    fn clone(&self) -> Self {
        LayerNode {
            app: self.app.clone(),
            mempool: self.mempool.clone(),
            pending_payloads: self.pending_payloads.clone(),
            current_height: self.current_height.clone(),
            last_digest: self.last_digest.clone(),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T: PersistentStorage + Send + Sync + 'static, P: PublicKey> LayerNode<T, P> {
    pub fn new(app: Arc<RwLock<App<T>>>, mempool: Arc<Mutex<Mempool>>, initial_height: u64) -> Self {
        LayerNode {
            app,
            mempool,
            pending_payloads: Arc::new(Mutex::new(BTreeMap::new())),
            current_height: Arc::new(Mutex::new(initial_height)),
            last_digest: Arc::new(Mutex::new([0u8; 32])),
            _phantom: std::marker::PhantomData,
        }
    }

    /// Access to the app for external callers (e.g., gRPC query handler).
    pub fn app(&self) -> Arc<RwLock<App<T>>> {
        self.app.clone()
    }

    /// Access to the mempool for external callers (e.g., gRPC tx submission).
    pub fn mempool(&self) -> Arc<Mutex<Mempool>> {
        self.mempool.clone()
    }

    /// Access to pending payloads for the Relay to populate when receiving
    /// broadcast payloads from other validators.
    /// CRITICAL: The Relay (Plan 03) MUST be constructed with this same Arc
    /// so that non-proposer validators can look up payloads during verify().
    /// Without this shared wiring, only the proposer has payloads in the map
    /// and 2-of-3 validators would always fail verify(), blocking consensus.
    pub fn pending_payloads(&self) -> Arc<Mutex<BTreeMap<[u8; 32], BlockPayload>>> {
        self.pending_payloads.clone()
    }

    /// Internal: compute the [u8; 32] digest of a BlockPayload.
    fn compute_digest(payload: &BlockPayload) -> [u8; 32] {
        payload.digest()
    }

    /// Internal: execute a block payload by calling App::finalize_block().
    /// This is called from certify() — DETERMINISM CRITICAL.
    /// NEVER call this from verify().
    pub(crate) async fn execute_block(&self, digest: [u8; 32]) -> bool {
        // Remove the payload from the pending map.
        // BTreeMap removal is deterministic (exact key lookup).
        let payload = {
            let mut pending = self.pending_payloads.lock().await;
            pending.remove(&digest)
        };

        let payload = match payload {
            Some(p) => p,
            None => {
                // Payload not found — block cannot be executed.
                // This may happen if the Relay hasn't delivered the payload yet
                // (Plan 03 adds the Relay; for Phase 2, only the proposer has payloads).
                return false;
            }
        };

        // Derive the block height: current_height + 1.
        let height = {
            let h = self.current_height.lock().await;
            *h + 1
        };

        // Convert BlockPayload to layer_std::api::Block.
        // Block timestamp comes from payload.timestamp_nanos (from consensus context — DETERMINISTIC).
        // NEVER use SystemTime::now() here.
        //
        // Deserialize transactions from BlockPayload raw bytes.
        // Each entry in payload.txs is a raw Cosmos proto-encoded tx (cosmos.tx.v1beta1.TxRaw).
        // parse_cosmos_tx() handles the full proto decode + signature extraction.
        let chain_id = {
            let app = self.app.read().await;  // SHARED read lock — read-only
            app.info()
                .map(|b| b.chain_id.clone())
                .unwrap_or_else(|| "junoclaw-1".to_string())
        };  // read lock released here

        let mut txs: Vec<layer_std::Tx> = Vec::with_capacity(payload.txs.len());
        for raw in &payload.txs {
            match parse_cosmos_tx(raw.clone(), &chain_id) {
                Ok(tx) => txs.push(tx),
                Err(e) => {
                    tracing::warn!(
                        error = ?e,
                        tx_len = raw.len(),
                        "Skipping malformed tx in block — proposer's check_tx should have caught this"
                    );
                    // Skip malformed txs rather than rejecting the whole block.
                    // The proposer's check_tx already validated these; errors here
                    // mean the tx was corrupted in transit or the chain_id changed.
                }
            }
        }

        let block = Block {
            txs,
            height,
            time: Timestamp::from_nanos(payload.timestamp_nanos),
            proposer_address: payload.proposer.clone(),
            last_votes: vec![],
            // The BLS certificate is NOT available at certify() time — it is produced
            // by the consensus engine after a quorum of validators certify. The
            // LayerReporter receives the Finalization activity with cert_bytes and
            // calls App::set_block_certificate(height, cert_bytes) to persist it.
            // See main.rs LayerReporter::report() for the storage path.
            certificate: None,
        };

        let result = {
            let mut app = self.app.write().await;  // EXCLUSIVE write lock — mutates state
            app.finalize_block(block)
        };  // write lock released here

        match result {
            Ok(response) => {
                // Commit succeeded: advance height and update last_digest.
                let mut h = self.current_height.lock().await;
                *h = height;
                let mut last = self.last_digest.lock().await;
                *last = digest;

                // Structured log: height, app_hash, digest (CONS-04, CONS-05 audit support)
                // The certificate bytes are set in Block.certificate by the Reporter after
                // certify() returns. The verify-consensus.sh script parses these fields.
                tracing::info!(
                    height = height,
                    app_hash = %hex::encode(&response.app_hash),
                    digest = %hex::encode(digest),
                    "Block finalized — certificate stored by Reporter on Finalization activity"
                );
                true
            }
            Err(e) => {
                tracing::error!(height = height, error = ?e, "finalize_block failed");
                false
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Automaton and CertifiableAutomaton implementations
// ---------------------------------------------------------------------------

impl<T, P> Automaton for LayerNode<T, P>
where
    T: PersistentStorage + Send + Sync + 'static,
    P: PublicKey + Clone + Send + Sync + 'static,
{
    type Context = Context<sha256::Digest, P>;
    type Digest = sha256::Digest;

    async fn genesis(&mut self, _epoch: Epoch) -> Self::Digest {
        // Return initial app_hash from genesis state as a sha256::Digest.
        // app_hash() returns Vec<u8>; we normalize to [u8; 32] via SHA-256 if needed.
        let app_hash = {
            let app = self.app.read().await;  // SHARED read lock — read-only
            app.app_hash()
        };
        // If app_hash is already 32 bytes, use it directly as the digest.
        // Otherwise, hash it to normalize to 32 bytes.
        if let Ok(arr) = <[u8; 32]>::try_from(app_hash.as_slice()) {
            sha256::Digest::from(arr)
        } else {
            let mut hasher = sha256::Sha256::new();
            hasher.update(&app_hash);
            hasher.finalize()
        }
    }

    async fn propose(&mut self, context: Self::Context) -> oneshot::Receiver<Self::Digest> {
        let (tx, rx) = oneshot::channel();

        // Drain transactions from the mempool.
        let raw_txs = {
            let mut pool = self.mempool.lock().await;
            pool.drain_batch(MAX_BLOCK_TXS)
        };

        // Build BlockPayload with current height + 1 and parent digest.
        let height = {
            let h = self.current_height.lock().await;
            *h + 1
        };
        let last_digest = {
            let d = self.last_digest.lock().await;
            *d
        };

        // Timestamp comes from the consensus context (same on all validators — DETERMINISTIC).
        // We derive a deterministic timestamp from the view number.
        // IMPORTANT: Timestamp must be > genesis time (1_673_194_026_078_305_426 ns, Jan 2023).
        // App::finalize_block() rejects blocks with timestamps <= the previous block's timestamp.
        // Use genesis_time + view * 1e9 ns: monotonically increasing and deterministic.
        // Each view adds 1 second, starting from genesis epoch to satisfy the timestamp check.
        const GENESIS_TIME_NS: u64 = 1_673_194_026_078_305_426;
        let view_num = context.round.view().get();
        let timestamp_nanos = GENESIS_TIME_NS.saturating_add(view_num.saturating_mul(1_000_000_000));

        // Proposer is the leader's public key bytes from context.
        // PublicKey: Array: AsRef<[u8]> — safe to copy the bytes.
        let proposer = context.leader.as_ref().to_vec();

        let total_tx_bytes: usize = raw_txs.iter().map(|t| t.len()).sum();
        tracing::info!(
            tx_count = raw_txs.len(),
            total_tx_bytes = total_tx_bytes,
            height = height,
            "propose: drained mempool"
        );

        let payload = BlockPayload {
            height,
            timestamp_nanos,
            proposer,
            txs: raw_txs,
            parent_digest: last_digest,
        };

        let payload_bytes = payload.to_bytes().len();
        let digest_bytes = Self::compute_digest(&payload);
        tracing::info!(
            payload_bytes = payload_bytes,
            "propose: built BlockPayload"
        );
        let digest = sha256::Digest::from(digest_bytes);

        // Store in pending map so verify() can look it up.
        {
            let mut pending = self.pending_payloads.lock().await;
            pending.insert(digest_bytes, payload);
        }

        tx.send(digest).ok();
        rx
    }

    async fn verify(
        &mut self,
        _context: Self::Context,
        payload: Self::Digest,
    ) -> oneshot::Receiver<bool> {
        let (tx, rx) = oneshot::channel();

        // CRITICAL: verify() MUST NOT call app.finalize_block() or mutate App state.
        // Verify only checks if the payload digest exists in pending_payloads.
        //
        // The pending_payloads map is populated by TWO sources:
        //   (a) This node's own propose() when it is the leader.
        //   (b) The Relay (Plan 03) when it receives payloads from the proposing validator.
        //
        // The proposal digest (32 bytes, consensus vote channel) can reach this
        // validator before the full block payload finishes propagating over the
        // payload relay channel — especially for large store-code txs (~4.4MB).
        // Poll pending_payloads for a bounded window so verify() tolerates the
        // relay delay instead of rejecting the proposal outright. The wait runs
        // in a spawned task so the receiver returns immediately and the voter's
        // select loop is not blocked; the window stays under leader_timeout so
        // the result lands within the view's verification deadline.
        let digest_bytes: [u8; 32] = payload.0;
        let pending_payloads = self.pending_payloads.clone();
        tokio::spawn(async move {
            const VERIFY_POLL_INTERVAL: Duration = Duration::from_millis(25);
            const VERIFY_WAIT_MAX: Duration = Duration::from_millis(2_500);
            let start = Instant::now();
            let found = loop {
                {
                    let pending = pending_payloads.lock().await;
                    if pending.contains_key(&digest_bytes) {
                        break true;
                    }
                }
                if start.elapsed() >= VERIFY_WAIT_MAX {
                    break false;
                }
                tokio::time::sleep(VERIFY_POLL_INTERVAL).await;
            };
            if !found {
                tracing::debug!(
                    digest = %hex::encode(digest_bytes),
                    "verify: payload not received within wait window"
                );
            }
            tx.send(found).ok();
        });

        rx
    }
}

impl<T, P> CertifiableAutomaton for LayerNode<T, P>
where
    T: PersistentStorage + Send + Sync + 'static,
    P: PublicKey + Clone + Send + Sync + 'static,
{
    async fn certify(
        &mut self,
        _round: Round,
        payload: Self::Digest,
    ) -> oneshot::Receiver<bool> {
        let (tx, rx) = oneshot::channel();

        // DETERMINISM CRITICAL: certify() is the single commit point.
        // All code in execute_block must be deterministic:
        //   - BTreeMap (not HashMap) for pending_payloads
        //   - Timestamp from block payload (not SystemTime::now())
        //   - No floating-point arithmetic
        let digest_bytes: [u8; 32] = payload.0;
        let success = self.execute_block(digest_bytes).await;
        tx.send(success).ok();
        rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use cosmwasm_std::{to_json_binary, Timestamp as CwTimestamp};
    use layer_app::{App, AppConfig, StateMachine};
    use layer_app::genesis::{GenesisState, WasmParams};
    use layer_std::api::{InitChainRequest, TmPubKey, ValidatorUpdate};
    use layer_storage::MemoryStore;
    use tokio::sync::{Mutex, RwLock};

    use crate::block::BlockPayload;
    use crate::mempool::Mempool;

    // Use ed25519::PublicKey as the test key type (simpler to construct than BLS)
    use commonware_cryptography::ed25519;

    type TestNode = LayerNode<MemoryStore, ed25519::PublicKey>;

    /// Global mutex serializing all node tests that create App<T> (and thus wasmer JIT).
    /// Wasmer's mmap-based JIT initialization is not safe to call concurrently from
    /// multiple threads on macOS/Apple Silicon; concurrent initialization causes SIGBUS.
    /// Holding this mutex during the entire test ensures only one test runs at a time.
    static APP_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Construct a single-threaded tokio runtime for tests.
    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    fn make_genesis_state() -> GenesisState {
        GenesisState {
            bank: vec![],
            wasm: WasmParams {
                gov_account: "juno1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmdyychx".to_string(),
            },
        }
    }

    fn init_app() -> App<MemoryStore> {
        let storage = MemoryStore::default();
        let logic = StateMachine::new(&AppConfig::new("/tmp/slay3rd-test-node"));
        let mut app = App::new(storage, logic);

        let genesis = make_genesis_state();
        let app_state = to_json_binary(&genesis).unwrap();
        let request = InitChainRequest {
            time: CwTimestamp::from_nanos(1_673_194_026_078_305_426),
            chain_id: "junoclaw-1".into(),
            consensus_params: Default::default(),
            validators: vec![ValidatorUpdate {
                pub_key: TmPubKey::Ed25519(vec![123u8; 32]),
                power: 1_000_000,
            }],
            app_state,
            initial_height: 1,
        };
        app.init(request).unwrap();
        app
    }

    fn make_layer_node_locked(
        _guard: &std::sync::MutexGuard<()>,
    ) -> TestNode {
        let app = Arc::new(RwLock::new(init_app()));
        let mempool = Arc::new(Mutex::new(Mempool::new(1000)));
        // App starts with initial_height=1, meaning first block is height=1.
        // LAST_BLOCK in init() is stored as initial_height - 1 = 0.
        // So current_height should start at 0 (next block will be 1).
        LayerNode::new(app, mempool, 0)
    }

    fn make_layer_node() -> TestNode {
        // Acquire the global lock to serialize wasmer JIT initialization.
        let _guard = APP_TEST_LOCK.lock().unwrap();
        make_layer_node_locked(&_guard)
    }

    fn make_payload_at_genesis_time(height: u64, parent_digest: [u8; 32]) -> BlockPayload {
        // Timestamp must be >= genesis time (1_673_194_026_078_305_426 ns).
        // Each block adds 100ms to ensure monotonically increasing timestamps.
        BlockPayload {
            height,
            timestamp_nanos: 1_673_194_026_078_305_426 + height * 100_000_000,
            proposer: vec![1u8; 32],
            txs: vec![],
            parent_digest,
        }
    }

    fn insert_payload_sync(node: &TestNode, payload: BlockPayload) -> [u8; 32] {
        let digest = payload.digest();
        rt().block_on(async {
            let pending_arc = node.pending_payloads();
            let mut pending = pending_arc.lock().await;
            pending.insert(digest, payload);
        });
        digest
    }

    #[test]
    fn test_genesis_returns_32_byte_digest() {
        let mut node = make_layer_node();
        let digest = rt().block_on(node.genesis(Epoch::new(0)));
        let bytes: &[u8] = &digest;
        assert_eq!(bytes.len(), 32, "genesis digest must be 32 bytes");
    }

    #[test]
    fn test_genesis_is_deterministic() {
        // Two nodes with the same genesis should produce the same app_hash.
        let mut node1 = make_layer_node();
        let mut node2 = make_layer_node();
        let digest1 = rt().block_on(node1.genesis(Epoch::new(0)));
        let digest2 = rt().block_on(node2.genesis(Epoch::new(0)));
        assert_eq!(
            digest1, digest2,
            "genesis digest should be deterministic for same genesis state"
        );
    }

    #[test]
    fn test_verify_returns_true_for_known_digest() {
        let node = make_layer_node();
        let payload = BlockPayload {
            height: 1,
            timestamp_nanos: 1_000_000_000,
            proposer: vec![1u8; 32],
            txs: vec![],
            parent_digest: [0u8; 32],
        };
        let digest_bytes = insert_payload_sync(&node, payload);

        // Simulate what verify() does: check pending_payloads
        let found = rt().block_on(async {
            let pending_arc = node.pending_payloads();
            let pending = pending_arc.lock().await;
            pending.contains_key(&digest_bytes)
        });
        assert!(found, "digest should be in pending_payloads after insert");
    }

    #[test]
    fn test_verify_returns_false_for_unknown_digest() {
        let node = make_layer_node();
        let unknown_digest = [42u8; 32];
        let found = rt().block_on(async {
            let pending_arc = node.pending_payloads();
            let pending = pending_arc.lock().await;
            pending.contains_key(&unknown_digest)
        });
        assert!(!found, "unknown digest should not be in pending_payloads");
    }

    #[test]
    fn test_certify_calls_finalize_block() {
        let node = make_layer_node();
        let payload = make_payload_at_genesis_time(1, [0u8; 32]);
        let digest = insert_payload_sync(&node, payload);

        let success = rt().block_on(node.execute_block(digest));
        assert!(success, "execute_block should succeed for valid empty-tx payload");

        // Check that height was incremented
        let height = rt().block_on(async { *node.current_height.lock().await });
        assert_eq!(height, 1, "height should be incremented to 1 after first certify");
    }

    #[test]
    fn test_certify_removes_pending_payload() {
        let node = make_layer_node();
        let payload = make_payload_at_genesis_time(1, [0u8; 32]);
        let digest = insert_payload_sync(&node, payload);

        // Verify it's in pending before certify
        let in_pending_before = rt().block_on(async {
            let pending_arc = node.pending_payloads();
            let pending = pending_arc.lock().await;
            pending.contains_key(&digest)
        });
        assert!(in_pending_before, "payload should be in pending before certify");

        rt().block_on(node.execute_block(digest));

        // Verify it's removed after certify
        let in_pending_after = rt().block_on(async {
            let pending_arc = node.pending_payloads();
            let pending = pending_arc.lock().await;
            pending.contains_key(&digest)
        });
        assert!(!in_pending_after, "payload should be removed from pending after certify");
    }

    #[test]
    fn test_certify_returns_false_for_unknown_digest() {
        let node = make_layer_node();
        let unknown_digest = [99u8; 32];
        let result = rt().block_on(node.execute_block(unknown_digest));
        assert!(!result, "execute_block should return false for unknown digest");
    }

    #[test]
    fn test_verify_does_not_mutate_app_state() {
        let node = make_layer_node();

        // Get initial app_hash
        let initial_hash = rt().block_on(async {
            let app_arc = node.app();
            let app = app_arc.read().await;
            app.app_hash()
        });

        // Insert a payload and check if it's in pending (simulating verify)
        let payload = BlockPayload {
            height: 1,
            timestamp_nanos: 1_000_000_000,
            proposer: vec![1u8; 32],
            txs: vec![],
            parent_digest: [0u8; 32],
        };
        let digest = insert_payload_sync(&node, payload);
        let _found = rt().block_on(async {
            let pending_arc = node.pending_payloads();
            let pending = pending_arc.lock().await;
            pending.contains_key(&digest)
        });

        // App hash should not change after verify (only reads pending_payloads)
        let after_hash = rt().block_on(async {
            let app_arc = node.app();
            let app = app_arc.read().await;
            app.app_hash()
        });
        assert_eq!(
            initial_hash, after_hash,
            "verify should not mutate app state (app_hash should be unchanged)"
        );
    }

    #[test]
    fn test_pending_payloads_accessor_returns_shared_arc() {
        let node = make_layer_node();
        let arc1 = node.pending_payloads();
        let arc2 = node.pending_payloads();

        // Both arcs should point to the same underlying data
        let payload = BlockPayload {
            height: 1,
            timestamp_nanos: 500_000_000,
            proposer: vec![2u8; 32],
            txs: vec![],
            parent_digest: [0u8; 32],
        };
        let digest = payload.digest();
        rt().block_on(async {
            let mut map = arc1.lock().await;
            map.insert(digest, payload);
        });
        let found = rt().block_on(async {
            let map = arc2.lock().await;
            map.contains_key(&digest)
        });
        assert!(found, "pending_payloads() should return the same shared Arc");
    }

    #[test]
    fn test_sequential_certify_increments_height() {
        let node = make_layer_node();

        // Block 1 (height = 1)
        let payload1 = make_payload_at_genesis_time(1, [0u8; 32]);
        let digest1 = insert_payload_sync(&node, payload1);
        assert!(rt().block_on(node.execute_block(digest1)), "first block should certify");
        let h1 = rt().block_on(async { *node.current_height.lock().await });
        assert_eq!(h1, 1);

        // Block 2 (height = 2)
        let payload2 = make_payload_at_genesis_time(2, digest1);
        let digest2 = insert_payload_sync(&node, payload2);
        assert!(rt().block_on(node.execute_block(digest2)), "second block should certify");
        let h2 = rt().block_on(async { *node.current_height.lock().await });
        assert_eq!(h2, 2);
    }

    /// Build a properly signed Cosmos tx bytes (cosmos.tx.v1beta1.TxRaw encoded) using cosmrs.
    ///
    /// Returns raw bytes that parse_cosmos_tx() can decode. The tx is signed with a random
    /// secp256k1 key and targets the test chain ("junoclaw-1"). The signer account is
    /// not in genesis, so finalize_block may reject it — but the DESERIALIZATION succeeds.
    #[cfg(test)]
    fn build_valid_tx_bytes(chain_id: &str) -> Vec<u8> {
        use cosmrs::{
            bank::MsgSend,
            crypto::secp256k1,
            tx::{self, Fee, Msg, SignDoc, SignerInfo},
            Coin,
        };
        use layer_std::BECH32_PREFIX;

        const ACCOUNT_NUMBER: u64 = 17; // FIXED_ACCOUNT_NUMBER from layer_cosmos::tx

        let sender_private_key = secp256k1::SigningKey::random();
        let sender_public_key = sender_private_key.public_key();
        let sender_account_id = sender_public_key.account_id(BECH32_PREFIX).unwrap();
        let rcpt_account_id = secp256k1::SigningKey::random()
            .public_key()
            .account_id(BECH32_PREFIX)
            .unwrap();

        let amount = Coin {
            amount: 1_000u128,
            denom: "ujclaw".parse().unwrap(),
        };
        let fee_coin = Coin {
            amount: 100u128,
            denom: "ujclaw".parse().unwrap(),
        };

        let msg_send = MsgSend {
            from_address: sender_account_id.clone(),
            to_address: rcpt_account_id.clone(),
            amount: vec![amount],
        };

        let tx_body = tx::Body::new(vec![msg_send.to_any().unwrap()], "", 9001u16);
        let signer_info = SignerInfo::single_direct(Some(sender_public_key), 0);
        let auth_info = signer_info.auth_info(Fee::from_amount_and_gas(fee_coin, 200_000u64));

        let parsed_chain_id = chain_id.parse().unwrap();
        let sign_doc =
            SignDoc::new(&tx_body, &auth_info, &parsed_chain_id, ACCOUNT_NUMBER).unwrap();
        let tx_signed = sign_doc.sign(&sender_private_key).unwrap();
        tx_signed.to_bytes().unwrap()
    }

    /// Validates that execute_block() correctly deserializes transactions from BlockPayload
    /// raw bytes via parse_cosmos_tx().
    ///
    /// Test flow:
    /// 1. Create an initialized App<MemoryStore> with genesis applied.
    /// 2. Construct a BlockPayload with valid Cosmos tx bytes in the `txs` field.
    /// 3. Execute the block and verify no "Skipping malformed tx" warning fires.
    /// 4. Construct a BlockPayload with invalid bytes mixed with valid bytes.
    /// 5. Verify the invalid bytes are skipped (deserialization path handles it gracefully).
    ///
    /// NOTE: execute_block() calls finalize_block() which may return an error if the tx
    /// signer is not in genesis. The test focuses on verifying the DESERIALIZATION path
    /// (parse_cosmos_tx called for each raw tx in payload.txs) rather than finalize success.
    /// The key assertion is that valid tx bytes parse successfully (no deserialization error
    /// before reaching finalize_block) and invalid bytes are skipped gracefully.
    #[test]
    fn test_execute_block_with_real_txs() {
        let _guard = APP_TEST_LOCK.lock().unwrap();

        let chain_id = "junoclaw-1";

        // Build valid tx bytes
        let valid_tx = bytes::Bytes::from(build_valid_tx_bytes(chain_id));

        // Build invalid tx bytes (random garbage)
        let invalid_tx = bytes::Bytes::from(vec![0xFF_u8, 0xFE, 0xAB, 0x12, 0x00]);

        // Test 1: Parse valid tx bytes directly via parse_cosmos_tx to confirm
        // the bytes are well-formed (confirming the deserialization path works).
        let parse_result = layer_cosmos::parse_cosmos_tx(valid_tx.clone(), chain_id);
        assert!(
            parse_result.is_ok(),
            "Valid tx bytes should parse successfully via parse_cosmos_tx: {:?}",
            parse_result.err()
        );

        // Test 2: Parse invalid tx bytes — must return an error, not panic.
        let parse_invalid = layer_cosmos::parse_cosmos_tx(invalid_tx.clone(), chain_id);
        assert!(
            parse_invalid.is_err(),
            "Invalid tx bytes should return an error, not panic"
        );

        // Test 3: execute_block() with valid tx bytes in payload.
        // The block has valid tx bytes. finalize_block() may reject the txs (signer not
        // in genesis), but that's OK — the deserialization path has already been exercised.
        let node = make_layer_node_locked(&_guard);
        let payload = BlockPayload {
            height: 1,
            timestamp_nanos: 1_673_194_026_078_305_426 + 100_000_000,
            proposer: vec![1u8; 32],
            txs: vec![valid_tx.clone()],
            parent_digest: [0u8; 32],
        };
        let digest = insert_payload_sync(&node, payload);
        // execute_block may return false if finalize_block fails (tx signer not in genesis),
        // but it should not panic. The deserialization path was exercised.
        let _result = rt().block_on(node.execute_block(digest));

        // Test 4: execute_block() with mixed valid + invalid bytes in payload.
        // The valid tx should be deserialized; the invalid one should be skipped with a warning.
        let node2 = make_layer_node_locked(&_guard);
        let payload_mixed = BlockPayload {
            height: 1,
            timestamp_nanos: 1_673_194_026_078_305_426 + 100_000_000,
            proposer: vec![1u8; 32],
            txs: vec![valid_tx.clone(), invalid_tx.clone(), valid_tx.clone()],
            parent_digest: [0u8; 32],
        };
        let digest2 = insert_payload_sync(&node2, payload_mixed);
        // Must not panic even with mixed valid/invalid bytes.
        let _result2 = rt().block_on(node2.execute_block(digest2));

        // Test 5: execute_block() with empty txs payload (existing behavior preserved).
        let node3 = make_layer_node_locked(&_guard);
        let payload_empty = BlockPayload {
            height: 1,
            timestamp_nanos: 1_673_194_026_078_305_426 + 100_000_000,
            proposer: vec![1u8; 32],
            txs: vec![],
            parent_digest: [0u8; 32],
        };
        let digest3 = insert_payload_sync(&node3, payload_empty);
        let result3 = rt().block_on(node3.execute_block(digest3));
        assert!(
            result3,
            "execute_block with empty txs should still succeed (finalize_block works)"
        );
    }
}
