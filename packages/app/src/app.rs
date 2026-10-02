use core::str;

use sha2::{Digest as Sha2Digest, Sha256};
use thiserror::Error;
use tracing::{
    debug, debug_span,
    field::{debug as dbg, display, Empty},
    info, info_span,
};

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{BlockInfo, Coin, Decimal, StdError, Uint128};

use crate::genesis::GenesisState;
use crate::sm::StateMachine;
use crate::{
    auth::TxData,
    error::{PulsarError, PulsarResult},
};
use layer_std::response::QueryResponse;
use layer_std::{
    api::{
        Block, BlockParams, FinalizeBlockResponse, GasInfo, InitChainRequest, InitChainResponse,
        TxResponse, TxResult,
    },
    HexEncode, TxError,
};
use layer_std::{GasMeter, Query, Rfc3339, Tx};
use layer_storage::{
    atomic, prefixed, prefixed_read, Item, PersistentStorage, ReadonlyStorage, ScratchTx, Storage,
    Transaction,
};

// FIXME: make this configurable on per-node basis
// Query gas is node-local (never consensus). Must exceed the heaviest
// legitimate smart query: in-contract MAYO-5 verify ≈ 726k SDK gas.
// 4M gives ~5× headroom while still bounding per-query CPU burn.
const DEFAULT_QUERY_GAS: u64 = 4_000_000;
const DEFAULT_SIMULATE_GAS: u64 = 10_000_000;

// these are all consensus critical and must be identical over all nodes
// FIXME: init them from genesis and store them somewhere
const MAX_VALIDATE_GAS: u64 = 200_000;
const MAX_BEGIN_BLOCK_GAS: u64 = 10_000_000;
const MAX_END_BLOCK_GAS: u64 = 10_000_000;

pub const GAS_COST_TX_BYTE: u64 = 10;

/// Validator-local minimum gas price — a mempool admission filter applied in
/// `check_tx` only (NOT in deliver). This mirrors Cosmos SDK `minimum-gas-prices`:
/// it is a per-validator policy, not a consensus rule, so validators may set
/// different floors without diverging. It prevents fee-less spam from entering
/// the mempool and prices block space. DeliverTx still deducts whatever fee was
/// set; it does not re-check the floor.
///
/// Parsed from a `<decimal><denom>` string like `"0.001ujclaw"`. A zero price
/// disables the floor (accepts any fee, including none).
#[derive(Debug, Clone)]
pub struct MinGasPrice {
    /// Fee per unit of gas (e.g. 0.001 ujclaw/gas).
    pub price: Decimal,
    /// The denom the fee must be paid in.
    pub denom: String,
}

impl MinGasPrice {
    /// Parse `"<decimal><denom>"` (e.g. `"0.001ujclaw"`). Returns `None` if the
    /// string is empty or malformed. A `"0<denom>"` price yields a struct whose
    /// `required_fee` is always 0 (floor disabled).
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        // Split at the first non-numeric, non-dot character => "<decimal><denom>".
        let split = s
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(s.len());
        let (price_str, denom) = s.split_at(split);
        if denom.is_empty() {
            return None;
        }
        let price = if price_str.is_empty() {
            Decimal::zero()
        } else {
            price_str.parse::<Decimal>().ok()?
        };
        Some(MinGasPrice {
            price,
            denom: denom.to_string(),
        })
    }

    /// Minimum fee (in `denom`) required for `gas_wanted` gas: ceil(price * gas).
    pub fn required_fee(&self, gas_wanted: u64) -> Uint128 {
        // price.atomics() = price * 10^18. required = ceil(atomics * gas / 10^18).
        // Decimal::one().atomics() == 10^18 (DECIMAL_FRACTIONAL is private).
        let frac = Decimal::one().atomics().u128();
        let atomics = self.price.atomics().u128();
        let req = (atomics.saturating_mul(gas_wanted as u128) + (frac - 1)) / frac;
        Uint128::from(req)
    }

    /// Returns `Err(InsufficientFee)` if `fee` does not cover the floor for
    /// `gas_wanted` gas in the expected denom.
    pub fn check(&self, fee: &Option<Coin>, gas_wanted: u64) -> Result<(), TxError> {
        let required = self.required_fee(gas_wanted);
        let provided = fee.as_ref().map(|c| c.amount.u128()).unwrap_or(0);
        let denom_ok = fee.as_ref().map(|c| c.denom == self.denom).unwrap_or(false);
        if required.is_zero() || (denom_ok && provided >= required.u128()) {
            Ok(())
        } else {
            Err(TxError::InsufficientFee {
                required: required.u128(),
                provided,
                denom: self.denom.clone(),
                gas_wanted,
            })
        }
    }
}

/// This maintains all application global state and is a framework-agnostic entrypoint for the
/// application. It *should* be able to run inside an ABCI app as well as an Avalanche Subnet.
///
/// We assume this is wrapped in `Arc<RwLock<App>>` if done in multi-threaded context.
/// Some methods require &mut.
/// We also do a two-step creation. `::new()` constructs an App with no state (which can be stored
/// in the `Arc<RwLock<_>>`), and we need `&mut App` to `init()` or `load_from_storage()` to set
/// up the inner state.
/// All other methods require this inner state to be set up and will panic otherwise.
#[derive(Debug)]
pub struct App<T: PersistentStorage> {
    // State
    pub(crate) storage: T,

    // State Machine Logic
    pub(crate) logic: StateMachine,

    /// Validator-local minimum gas price (mempool admission filter, CheckTx
    /// only). `None` disables the floor. Not part of consensus state.
    min_gas_price: Option<MinGasPrice>,

    data: Option<InnerData>,
}

#[derive(Debug, Clone)]
struct InnerData {
    // Current Block
    block: BlockInfo,

    // cached chain_id
    chain_id: String,

    // FIXME: later these may become mutable
    params: BlockParams,
}

#[derive(Error, Debug, PartialEq)]
pub enum AppLoadError {
    #[error("No State Stored")]
    NoStoredState,

    #[error("Invalid State: {0}")]
    InvalidState(String),
}

pub const NAMESPACE_APP: &[u8] = b"app";
const APP_STATE: Item<AppState> = Item::new("state");
// _ prefix ensures it is not included in the app hash
const LAST_BLOCK: Item<BlockInfo> = Item::new("_last_block");

/// Storage key prefix for BLS certificates keyed by block height.
/// The "_" prefix follows the same convention as LAST_BLOCK — it excludes
/// the certificate from app_hash computation. Certificate delivery timing
/// differs across validators and must not affect consensus determinism.
const BLOCK_CERTIFICATE_KEY_PREFIX: &str = "_cert/";

/// Storage key prefix for encoded consensus `Proposal` bytes, keyed by block
/// height. Persisted alongside the BLS certificate so that light clients
/// (08-wasm BLS light client) can reconstruct the exact signed message
/// (`encode(Proposal { round, parent, payload })`) without re-deriving it
/// from separate fields. Same "_" app_hash-exclusion convention as LAST_BLOCK
/// and BLOCK_CERTIFICATE_KEY_PREFIX.
const BLOCK_PROPOSAL_KEY_PREFIX: &str = "_proposal/";

/// Storage key prefix for block timestamps (nanoseconds since UNIX epoch),
/// keyed by block height. Served to light client relayers alongside the
/// certificate and proposal so they can build 08-wasm headers (IBC
/// timestamp semantics: nanoseconds). Same "_" app_hash-exclusion
/// convention as the certificate and proposal prefixes.
const BLOCK_TIMESTAMP_KEY_PREFIX: &str = "_ts/";

/// Storage key prefix for the full bincode-serialized `BlockPayload`, keyed
/// by block height. Persisted at execute_block time so that membership
/// proofs can carry the exact payload bytes — the light client recomputes
/// `sha256(payload_bytes)` and compares it to the consensus state's
/// `payload_digest`, then extracts `state_root` from the parsed payload.
/// Same "_" app_hash-exclusion convention.
const BLOCK_PAYLOAD_KEY_PREFIX: &str = "_payload/";

/// Storage key prefix for the persisted tx-result index (Q2). Values are
/// prost-encoded `cosmos.base.abci.v1beta1.TxResponse` keyed by the
/// upper-case hex txhash. Same "_" app_hash-exclusion convention — this is
/// node-local indexing state, never part of consensus.
const TX_RESPONSE_KEY_PREFIX: &str = "_txres/";

/// Storage item holding the latest committed state root — the Merkle root
/// over all non-`'_'`-prefixed KV entries, recomputed at the end of every
/// `finalize_block` (and at `init` for the genesis state). The next block's
/// `BlockPayload.state_root` carries this value, giving the signed payload
/// a commitment to the application state one block back (Tendermint
/// app-hash semantics). "_" prefix excludes it from app_hash and from the
/// state root computation itself.
const STATE_ROOT: Item<[u8; 32]> = Item::new("_state_root");

#[cw_serde]
pub struct AppState {
    pub chain_id: String,

    pub params: BlockParams,
}

/// A Merkle membership proof for a single key in committed app state.
/// Served by the `layer.lightclient.v1.Query/Proof` RPC; the relayer pairs
/// it with `BlockPayload.state_root` from the block at `state_height + 1`
/// to assemble the contract-side proof.
#[derive(Debug, Clone)]
pub struct StateProof {
    /// Height whose post-state this proof covers. The proof verifies
    /// against the consensus state at `state_height + 1`.
    pub state_height: u64,
    /// The proven storage key.
    pub key: Vec<u8>,
    /// The proven value at `key`.
    pub value: Vec<u8>,
    /// Index of the leaf in the sorted leaf list.
    pub leaf_index: u64,
    /// Sibling hashes bottom-up; `None` = promotion level (odd node count).
    pub siblings: Vec<Option<[u8; 32]>>,
}

// First step to creation
impl<T: PersistentStorage + 'static> App<T> {
    pub fn new(storage: T, logic: StateMachine) -> App<T> {
        App {
            storage,
            logic,
            min_gas_price: None,
            data: None,
        }
    }

    /// Set the validator-local minimum gas price (mempool admission filter).
    /// Call after `new`, before serving `check_tx`. `None` disables the floor.
    pub fn set_min_gas_price(&mut self, min_gas_price: Option<MinGasPrice>) {
        self.min_gas_price = min_gas_price;
    }
}

// Second creation step
impl<T: PersistentStorage + 'static> App<T> {
    /// Re-create a blockchain from existing stored state.
    /// If this fails with AppLoadError::NoStoredState, then we wait for init to be called.
    /// Otherwise we fail on loading.
    pub fn load_from_storage(&mut self) -> Result<(), AppLoadError> {
        let meter = GasMeter::infinite();
        let (state, block) = {
            let reader = self.storage.reader();
            let b = LAST_BLOCK
                .may_load(&reader, &meter)
                .map_err(|e| AppLoadError::InvalidState(e.to_string()))?;

            let app_store = prefixed_read(&reader, NAMESPACE_APP);
            let s = APP_STATE
                .may_load(&app_store, &meter)
                .map_err(|e| AppLoadError::InvalidState(e.to_string()))?;
            (s, b)
        };
        match (state, block) {
            (Some(state), Some(block)) => {
                debug!(?state, "Loaded state from storage");
                let data = InnerData {
                    block,
                    chain_id: state.chain_id,
                    params: state.params,
                };
                self.data = Some(data);
                Ok(())
            }
            _ => Err(AppLoadError::NoStoredState),
        }
    }

    /// Called once upon blockchain startup with genesis info, before anything else is called
    pub fn init(&mut self, request: InitChainRequest) -> PulsarResult<InitChainResponse> {
        let _span = debug_span!("app.init").entered();
        // Store the state
        let chain_id = request.chain_id.clone();
        let last_block = BlockInfo {
            // If initial height is 10, that means the first block will be 10.
            // So, we store "last_block" as one less.
            height: request.initial_height.saturating_sub(1),
            time: request.time,
            chain_id: request.chain_id,
        };

        // start a transaction
        let mut writer = self.storage.writer();
        let meter = GasMeter::infinite();

        // Set up the state machine here
        let genesis = GenesisState::parse(&request.app_state)?;
        self.logic.init(&mut writer, &meter, &last_block, genesis)?;

        // Store the block info
        LAST_BLOCK.save(&mut writer, &meter, &last_block)?;
        // Store the application data
        let state = AppState {
            chain_id,
            params: request.consensus_params.block.clone(),
        };
        {
            // ensure we drop app_store before the commit
            let mut app_store = prefixed(&mut writer, NAMESPACE_APP);
            APP_STATE.save(&mut app_store, &meter, &state)?;
        }

        // commit to disk
        writer.commit(&meter)?;

        // Create the response
        self.data = Some(InnerData {
            block: last_block,
            chain_id: state.chain_id,
            params: state.params,
        });

        // Genesis state root — the first block's payload carries this so
        // membership proofs work from height 1 onward.
        self.store_state_root()?;

        Ok(InitChainResponse {
            consensus_params: request.consensus_params,
            validators: request.validators,
            app_hash: self.storage.app_hash(),
        })
    }
}

// All these require an initialized app and will panic if neither load_from_storage
// nor init have been successfully called before.
impl<T: PersistentStorage + 'static> App<T> {
    pub fn info(&self) -> Option<&BlockInfo> {
        self.data.as_ref().map(|d| &d.block)
    }

    pub fn app_hash(&self) -> Vec<u8> {
        self.storage.app_hash()
    }

    pub fn chain_id(&self) -> &str {
        &self.data.as_ref().unwrap().chain_id
    }

    /// Returns serialized response to the query that can be passed back verbatum
    pub fn query(&self, request: Query) -> PulsarResult<QueryResponse<PulsarError>> {
        let span = debug_span!("query", ?request, success = Empty, error = Empty).entered();
        let reader = self.storage.reader();

        // note, simulate needs different limit
        let meter = match &request {
            Query::Simulate(_) => self.simulate_gas_meter(),
            _ => self.query_gas_meter(),
        };

        let block = &self.data.as_ref().unwrap().block;
        let resp = self.logic.query(&reader, &meter, block, request);
        match &resp {
            Ok(response) => span.record("success", dbg(response)),
            Err(error) => span.record("error", display(error)),
        };
        reader.abort();
        resp
    }

    // initialize block gas meter from params, allow infinite if not set
    fn block_gas_meter(&self) -> GasMeter {
        let params = &self.data.as_ref().unwrap().params;
        params
            .max_gas
            .map(GasMeter::new)
            .unwrap_or_else(GasMeter::infinite)
    }

    // use block gas limit for simulations, or a default if not set
    fn simulate_gas_meter(&self) -> GasMeter {
        let params = &self.data.as_ref().unwrap().params;
        let limit = params.max_gas.unwrap_or(DEFAULT_SIMULATE_GAS);
        GasMeter::new(limit)
    }

    fn query_gas_meter(&self) -> GasMeter {
        GasMeter::new(DEFAULT_QUERY_GAS)
    }

    pub fn check_tx(&self, tx: Tx) -> TxResult<PulsarError> {
        let _span = debug_span!("check_tx").entered();
        // capture the declared fee before `tx` is moved into validate_tx
        let fee = match &tx {
            Tx::Signed(s) => s.fee.fee.clone(),
        };
        // temporary cache we will throw away
        let reader = self.storage.reader();
        let mut store = ScratchTx::new(&reader);

        // only run auth check — cap at MAX_VALIDATE_GAS so check_tx admits
        // exactly what deliver_tx can execute (audit: block meter here let
        // txs pass check that were guaranteed to fail at the 200k deliver cap)
        let meter = GasMeter::new(MAX_VALIDATE_GAS);
        let block = &self.data.as_ref().unwrap().block;
        let res = atomic(&mut store, &meter, |store, m| {
            self.logic.validate_tx(store, m, block, tx)
        });
        reader.abort();

        match res {
            Ok(TxData { gas_wanted, .. }) => {
                // Mempool admission: enforce the validator-local min gas price.
                // This is NOT a consensus rule — deliver does not re-check it.
                if let Some(mgp) = &self.min_gas_price {
                    if let Err(e) = mgp.check(&fee, gas_wanted) {
                        debug!(error = %e, "check_tx: below min gas price");
                        return TxResult {
                            gas: GasInfo::zero(),
                            result: Err(e.into()),
                        };
                    }
                }
                let gas_used = meter.used();
                let gas = GasInfo {
                    gas_used,
                    gas_wanted,
                };
                TxResult {
                    gas,
                    result: Ok(TxResponse::empty()),
                }
            }
            Err(e) => TxResult {
                gas: GasInfo::zero(),
                result: Err(e),
            },
        }
    }

    /// Run a tx for real against committed state — auth validation AND full
    /// message execution, metered — then throw the writes away. This is what
    /// backs the `Simulate` gRPC: gas numbers come from actually executing,
    /// not from `check_tx`-style auth-only validation.
    ///
    /// The tx runs in the context of the last committed block (SDK
    /// checkState semantics) on a `ScratchTx` overlay over a storage reader,
    /// so nothing persists and no commit/lock is taken.
    pub fn simulate(&self, tx: Tx) -> TxResult<PulsarError> {
        let _span = debug_span!("simulate").entered();
        let data = self.data.as_ref().unwrap();
        let block = data.block.clone();
        let reader = self.storage.reader();
        let mut store = ScratchTx::new(&reader);
        let block_meter = self.simulate_gas_meter();
        let res = self.execute_tx(&mut store, &block_meter, &block, tx);
        reader.abort();
        res
    }

    fn execute_tx(
        &self,
        storage: &mut dyn Storage,
        block_meter: &GasMeter,
        block: &BlockInfo,
        tx: Tx,
    ) -> TxResult<PulsarError> {
        // Abel: here is where we record execute_tx. Note that BlockInfo doesn't have block hash, but does have height.
        // I just added the tx_hash here
        let tx_hash = tx.tx_hash();
        let _span = debug_span!("execute_tx", ?tx, tx_hash = %HexEncode::new(&tx_hash), height = block.height).entered();
        // validate the transaction. if this passes, we commit the auth info (sequence / fee)
        // even if messages fail and are reverted
        let val_meter = GasMeter::new(MAX_VALIDATE_GAS);
        // we need to calculate this now before moving tx away
        let tx_byte_gas = tx.tx_len() * GAS_COST_TX_BYTE;
        let val_res = atomic(storage, &val_meter, |store, m| {
            self.logic.validate_tx(store, m, block, tx)
        });
        let data = match val_res {
            Ok(x) => x,
            Err(e) => {
                debug!(error = %e, "Tx auth error");
                // ignore this out of gas error, aborting anyway and future txs will fail
                let _ = block_meter.charge(val_meter.used());
                return TxResult {
                    gas: GasInfo::from_meter(&val_meter),
                    result: Err(e),
                };
            }
        };

        // prepare this tx-specific gas meter and charge for previous validation
        let gas_wanted = data.gas_wanted;
        let meter = GasMeter::new(gas_wanted);
        // charge for validation gas an also tx bytes
        if let Err(e) = meter.charge(val_meter.used() + tx_byte_gas) {
            // ignore this out of gas error, aborting anyway and future txs will fail
            let _ = block_meter.charge(val_meter.used());
            debug!(error = %e, "Tx auth error");
            return TxResult {
                gas: GasInfo::from_meter(&meter),
                result: Err(e.into()),
            };
        }

        // cap at some block limit - if more requested that fits in the block,
        // abort before trying to run the tx
        if gas_wanted > block_meter.remaining() {
            let err = PulsarError::ExceedsRemainingBlockGas {
                requested: gas_wanted,
                remaining: block_meter.remaining(),
            };
            debug!(error = %err, "Tx auth error");
            return TxResult {
                gas: GasInfo::from_meter(&meter),
                result: Err(err),
            };
        }

        // execute all messages atomically. if any fail, we don't write any state changes
        // from any of the messages
        // We don't wrap with AppMeter here, we allow state machine to do that (for eg wasm)x
        let resps: PulsarResult<Vec<_>> = atomic(storage, &meter, |store, m| {
            data.msgs
                .into_iter()
                .map(|msg| self.logic.process_msg(store, m, &data.signer, block, msg))
                .collect()
        });
        // we checked block limit above, we shouldn't fail here, so just ignore error
        let _ = block_meter.charge(meter.used());

        // collect responses
        let result = resps.map(|all| {
            // Two separate steps to combine data, then events.
            // Data is much cheaper to clone, so we do that first.
            let data = all.iter().map(|r| r.data.clone()).collect();
            let events = all.into_iter().map(|r| r.events).collect();
            TxResponse { data, events }
        });
        match &result {
            Ok(r) => debug!(success = ?r, "Tx success"),
            Err(e) => debug!(error = %e, "Tx error"),
        };

        // return result
        let gas = GasInfo::from_meter(&meter);
        // Log the error string on failure so the cause is visible at info level
        // (the detailed debug! above is filtered out under RUST_LOG=info).
        let err_str = match &result {
            Ok(_) => String::new(),
            Err(e) => e.to_string(),
        };
        info!(
            tx_hash = %HexEncode::new(&tx_hash),
            height = block.height,
            gas_used = gas.gas_used,
            gas_wanted = gas.gas_wanted,
            success = result.is_ok(),
            error = %err_str,
            "tx executed"
        );
        TxResult { gas, result }
    }

    pub fn finalize_block(
        &mut self,
        full_block: Block,
    ) -> PulsarResult<FinalizeBlockResponse<PulsarError>> {
        self.finalize_block_with_payload(full_block, None)
    }

    /// `finalize_block`, additionally persisting the consensus payload bytes
    /// under `_payload/{height}`. State writes, `LAST_BLOCK`, the state root
    /// and the payload land in ONE storage commit, so a crash can never leave
    /// a committed height without its state root or payload (either of which
    /// would wedge the node on restart).
    pub fn finalize_block_with_payload(
        &mut self,
        full_block: Block,
        payload: Option<Vec<u8>>,
    ) -> PulsarResult<FinalizeBlockResponse<PulsarError>> {
        let _span = info_span!(
            "finalize_block",
            height = full_block.height,
            block.time = %Rfc3339(full_block.time),
            block.nanos = full_block.time.nanos(),
            txs = full_block.txs.len(),
        )
        .entered();

        let mut writer = self.storage.writer();

        let data = self.data.as_ref().unwrap();

        // assert we are exactly one block ahead of last known state
        let block = BlockInfo {
            height: full_block.height,
            time: full_block.time,
            chain_id: data.chain_id.clone(),
        };
        let old_block = data.block.clone();
        if block.height != old_block.height + 1 {
            return Err(PulsarError::BadBlockHeight {
                got: block.height,
                previous: old_block.height,
            });
        }
        if block.time < old_block.time {
            return Err(PulsarError::DescendingBlockTime {
                got: old_block.time.nanos(),
                previous: block.time.nanos(),
            });
        }

        // Run begin block logic (not included in block gas)
        let begin_meter = GasMeter::new(MAX_BEGIN_BLOCK_GAS);
        let mut events = self
            .logic
            .begin_block(&mut writer, &begin_meter, &full_block)?;

        // Set the block gas meter to limit total gas usage by all txs
        let meter = self.block_gas_meter();
        // Execute all transactions within this global limit
        let tx_results: Vec<_> = full_block
            .txs
            .into_iter()
            .map(|tx| {
                // execute tx takes care of atomically committing or aborting auth and msg state writes
                self.execute_tx(&mut writer, &meter, &block, tx)
            })
            .collect();

        // Run end block logic (not included in block gas)
        let end_meter = GasMeter::new(MAX_END_BLOCK_GAS);
        let end_events = self.logic.end_block(&mut writer, &end_meter, &block)?;
        events.extend(end_events);

        // Update block data and commit to underlying store.
        // Use infinite gas meter to ensure we don't fail here
        let meter = GasMeter::infinite();
        LAST_BLOCK.save(&mut writer, &meter, &block)?;
        if let Some(bytes) = payload {
            let key = format!("{}{}", BLOCK_PAYLOAD_KEY_PREFIX, block.height);
            let payload_item: Item<Vec<u8>> = Item::new(&key);
            payload_item.save(&mut writer, &meter, &bytes)?;
        }

        // State root over the post-block state, computed through the writer
        // (which sees its own uncommitted writes). The NEXT block's payload
        // carries this root (app-hash semantics), which is what binds IBC
        // membership proofs to the signed certificate chain. Sidecar write —
        // excluded from app_hash and from the root itself.
        let root = state_root_over(&writer)?;
        STATE_ROOT.save(&mut writer, &meter, &root)?;
        writer.commit(&meter)?;

        // update block in cache
        self.data.as_mut().unwrap().block = block;

        Ok(FinalizeBlockResponse {
            events,
            tx_results,
            validator_updates: vec![],
            consensus_param_updates: None,
            app_hash: self.storage.app_hash(),
        })
    }

    /// Store a BLS12-381 threshold certificate for a previously committed block.
    ///
    /// Called by the consensus Reporter after the threshold signature is assembled
    /// from validator certify() votes. The certificate is NOT available at
    /// certify() time — it is produced by the consensus engine after a quorum
    /// of validators have certified the block.
    ///
    /// The certificate is stored under a "_cert/{height}" key. The "_" prefix
    /// excludes it from app_hash computation (same convention as LAST_BLOCK),
    /// because certificate delivery timing may differ across validators and
    /// must not affect consensus determinism.
    ///
    /// # Arguments
    /// * `height` - The block height this certificate belongs to
    /// * `certificate` - The raw BLS12-381 threshold signature bytes from the consensus engine
    ///
    /// # Returns
    /// `Ok(())` on success, or an error if the storage write fails.
    pub fn set_block_certificate(
        &mut self,
        height: u64,
        certificate: Vec<u8>,
    ) -> PulsarResult<()> {
        let meter = GasMeter::infinite();
        let mut writer = self.storage.writer();
        let key = format!("{}{}", BLOCK_CERTIFICATE_KEY_PREFIX, height);
        let cert_item: Item<Vec<u8>> = Item::new(&key);
        cert_item.save(&mut writer, &meter, &certificate)?;
        writer.commit(&meter)?;
        Ok(())
    }

    /// Retrieve the BLS certificate for a block at the given height, if stored.
    ///
    /// Returns `None` if no certificate has been stored for this height yet
    /// (e.g., the Reporter hasn't fired yet, or this is a genesis block).
    pub fn get_block_certificate(&self, height: u64) -> Option<Vec<u8>> {
        let meter = GasMeter::infinite();
        let reader = self.storage.reader();
        let key = format!("{}{}", BLOCK_CERTIFICATE_KEY_PREFIX, height);
        let cert_item: Item<Vec<u8>> = Item::new(&key);
        let result = cert_item.may_load(&reader, &meter).ok().flatten();
        reader.abort();
        result
    }

    /// Store the encoded consensus `Proposal` bytes for a previously committed
    /// block, alongside its BLS certificate.
    ///
    /// The `Proposal` (`round`, `parent`, `payload`) is exactly the message a
    /// BLS light client must reconstruct to verify `get_block_certificate`'s
    /// signature: `ops::verify_message::<MinSig>(pubkey, namespace,
    /// proposal_bytes, certificate)`. Without this, the light client cannot
    /// re-derive the signed message from height/timestamp/batch_hash alone.
    ///
    /// # Arguments
    /// * `height` - The block height this proposal belongs to
    /// * `proposal` - The commonware-codec encoded `Proposal<Sha256Digest>` bytes
    ///
    /// # Returns
    /// `Ok(())` on success, or an error if the storage write fails.
    pub fn set_block_proposal(&mut self, height: u64, proposal: Vec<u8>) -> PulsarResult<()> {
        let meter = GasMeter::infinite();
        let mut writer = self.storage.writer();
        let key = format!("{}{}", BLOCK_PROPOSAL_KEY_PREFIX, height);
        let proposal_item: Item<Vec<u8>> = Item::new(&key);
        proposal_item.save(&mut writer, &meter, &proposal)?;
        writer.commit(&meter)?;
        Ok(())
    }

    /// Retrieve the encoded `Proposal` bytes for a block at the given height,
    /// if stored. Returns `None` if no proposal has been stored for this
    /// height yet (e.g., the Reporter hasn't fired yet, or this is a genesis
    /// block).
    pub fn get_block_proposal(&self, height: u64) -> Option<Vec<u8>> {
        let meter = GasMeter::infinite();
        let reader = self.storage.reader();
        let key = format!("{}{}", BLOCK_PROPOSAL_KEY_PREFIX, height);
        let proposal_item: Item<Vec<u8>> = Item::new(&key);
        let result = proposal_item.may_load(&reader, &meter).ok().flatten();
        reader.abort();
        result
    }

    /// Store the block timestamp (nanoseconds since UNIX epoch) for a
    /// previously committed block, alongside its BLS certificate and
    /// consensus proposal.
    ///
    /// The 08-wasm light client header carries the block timestamp in IBC
    /// nanosecond semantics; relayers fetch it via this record to build
    /// `update_state` headers (packet timeout bookkeeping on the
    /// counterparty chain needs it).
    ///
    /// # Arguments
    /// * `height` - The block height this timestamp belongs to
    /// * `timestamp_nanos` - Block timestamp in nanoseconds since UNIX epoch
    ///
    /// # Returns
    /// `Ok(())` on success, or an error if the storage write fails.
    pub fn set_block_timestamp(&mut self, height: u64, timestamp_nanos: u64) -> PulsarResult<()> {
        let meter = GasMeter::infinite();
        let mut writer = self.storage.writer();
        let key = format!("{}{}", BLOCK_TIMESTAMP_KEY_PREFIX, height);
        let ts_item: Item<u64> = Item::new(&key);
        ts_item.save(&mut writer, &meter, &timestamp_nanos)?;
        writer.commit(&meter)?;
        Ok(())
    }

    /// Persist a finalized block's light-client record (BLS certificate,
    /// encoded consensus `Proposal`, timestamp) in ONE commit, so a crash can
    /// never leave a height with a certificate but no signed message (or
    /// vice versa). Sidecar keys only — no effect on app_hash / state root.
    pub fn set_block_finality(
        &mut self,
        height: u64,
        certificate: Vec<u8>,
        proposal: Vec<u8>,
        timestamp_nanos: u64,
    ) -> PulsarResult<()> {
        let meter = GasMeter::infinite();
        let mut writer = self.storage.writer();
        let cert_key = format!("{}{}", BLOCK_CERTIFICATE_KEY_PREFIX, height);
        Item::<Vec<u8>>::new(&cert_key).save(&mut writer, &meter, &certificate)?;
        let proposal_key = format!("{}{}", BLOCK_PROPOSAL_KEY_PREFIX, height);
        Item::<Vec<u8>>::new(&proposal_key).save(&mut writer, &meter, &proposal)?;
        let ts_key = format!("{}{}", BLOCK_TIMESTAMP_KEY_PREFIX, height);
        Item::<u64>::new(&ts_key).save(&mut writer, &meter, &timestamp_nanos)?;
        writer.commit(&meter)?;
        Ok(())
    }

    /// Retrieve the block timestamp (nanoseconds) for a block at the given
    /// height, if stored. Returns `None` if no timestamp has been stored for
    /// this height yet.
    pub fn get_block_timestamp(&self, height: u64) -> Option<u64> {
        let meter = GasMeter::infinite();
        let reader = self.storage.reader();
        let key = format!("{}{}", BLOCK_TIMESTAMP_KEY_PREFIX, height);
        let ts_item: Item<u64> = Item::new(&key);
        let result = ts_item.may_load(&reader, &meter).ok().flatten();
        reader.abort();
        result
    }

    /// Store the full bincode-serialized `BlockPayload` for a committed
    /// block. Called from `execute_block` (node.rs) after `finalize_block`
    /// succeeds — the payload is removed from `pending_payloads` there, so
    /// this is the only durable copy.
    ///
    /// Membership proofs carry these bytes: the light client recomputes
    /// `sha256(payload_bytes)` to match the consensus state's
    /// `payload_digest`, then parses the payload to extract `state_root`.
    pub fn set_block_payload(&mut self, height: u64, payload: Vec<u8>) -> PulsarResult<()> {
        let meter = GasMeter::infinite();
        let mut writer = self.storage.writer();
        let key = format!("{}{}", BLOCK_PAYLOAD_KEY_PREFIX, height);
        let payload_item: Item<Vec<u8>> = Item::new(&key);
        payload_item.save(&mut writer, &meter, &payload)?;
        writer.commit(&meter)?;
        Ok(())
    }

    /// Retrieve the serialized `BlockPayload` for a block at the given
    /// height, if stored.
    pub fn get_block_payload(&self, height: u64) -> Option<Vec<u8>> {
        let meter = GasMeter::infinite();
        let reader = self.storage.reader();
        let key = format!("{}{}", BLOCK_PAYLOAD_KEY_PREFIX, height);
        let payload_item: Item<Vec<u8>> = Item::new(&key);
        let result = payload_item.may_load(&reader, &meter).ok().flatten();
        reader.abort();
        result
    }

    /// Persist a prost-encoded `TxResponse` for `txhash` (Q2 — durable
    /// tx index). `txhash` is normalized (upper-case, `0x` stripped) by the
    /// caller-side `tx_index::normalize` convention.
    pub fn set_tx_response(&mut self, txhash: &str, encoded: &[u8]) -> PulsarResult<()> {
        let meter = GasMeter::infinite();
        let mut writer = self.storage.writer();
        let key = format!("{}{}", TX_RESPONSE_KEY_PREFIX, txhash);
        Item::<Vec<u8>>::new(&key).save(&mut writer, &meter, &encoded.to_vec())?;
        writer.commit(&meter)?;
        Ok(())
    }

    /// Retrieve the prost-encoded `TxResponse` for `txhash`, if indexed.
    pub fn get_tx_response(&self, txhash: &str) -> Option<Vec<u8>> {
        let meter = GasMeter::infinite();
        let reader = self.storage.reader();
        let key = format!("{}{}", TX_RESPONSE_KEY_PREFIX, txhash);
        let item: Item<Vec<u8>> = Item::new(&key);
        let result = item.may_load(&reader, &meter).ok().flatten();
        reader.abort();
        result
    }

    /// The latest committed state root (Merkle root over all non-`'_'` KV
    /// entries), as stored by the most recent `finalize_block`/`init`.
    /// `propose()` reads this to fill `BlockPayload.state_root`.
    /// Returns `None` before the first block is committed.
    pub fn state_root(&self) -> Option<[u8; 32]> {
        let meter = GasMeter::infinite();
        let reader = self.storage.reader();
        let result = STATE_ROOT.may_load(&reader, &meter).ok().flatten();
        reader.abort();
        result
    }

    /// Compute the Merkle root over all committed application state.
    ///
    /// Iterates every KV entry in the store, skipping `'_'`-prefixed sidecar
    /// keys (certificates, proposals, timestamps, payloads, the state root
    /// itself — the same exclusion `FastHasher` uses for app_hash), and
    /// builds a domain-separated binary Merkle tree over the sorted entries:
    ///
    ///   leaf = sha256(0x00 || key || value)
    ///   node = sha256(0x01 || left || right)   (odd nodes promote unchanged)
    ///
    /// The empty tree hashes to sha256 of the empty input. This MUST match
    /// the verifier in `contracts/light-client/src/merkle.rs` byte-for-byte.
    ///
    /// Cost is O(state size) per call — fine at devnet scale; the upgrade
    /// path is a versioned Merkle store (JMT/IAVL-style) when state grows.
    pub fn compute_state_root(&self) -> PulsarResult<[u8; 32]> {
        let reader = self.storage.reader();
        let root = state_root_over(&reader);
        reader.abort();
        root
    }

    /// Recompute the state root over committed state and persist it to the
    /// `_state_root` sidecar item. Called at the end of `finalize_block`
    /// (post-commit) and at `init` (post-genesis).
    fn store_state_root(&mut self) -> PulsarResult<()> {
        let root = self.compute_state_root()?;
        let meter = GasMeter::infinite();
        let mut writer = self.storage.writer();
        STATE_ROOT.save(&mut writer, &meter, &root)?;
        writer.commit(&meter)?;
        Ok(())
    }

    /// Build a Merkle membership proof for `key` over the latest committed
    /// state, all within one storage snapshot (the reported `state_height`
    /// comes from `LAST_BLOCK` in the same reader, so it always matches the
    /// proven state even if a block commits mid-request).
    ///
    /// Returns `None` if the key does not exist in committed state
    /// (non-membership proofs are not supported — they need a versioned
    /// tree; see BLS_LIGHT_CLIENT_SPEC §8).
    ///
    /// The proof verifies against `BlockPayload.state_root` of the block at
    /// `state_height + 1` — the relayer fetches that payload via the
    /// `Block` RPC and assembles the contract-side proof.
    pub fn state_proof(&self, key: &[u8]) -> PulsarResult<Option<StateProof>> {
        let meter = GasMeter::infinite();
        let reader = self.storage.reader();

        // Height at snapshot time — read from the same reader as the leaves
        // so the proof and the height can never disagree.
        let state_height = LAST_BLOCK
            .may_load(&reader, &meter)?
            .map(|b| b.height)
            .unwrap_or(0);

        let iter = reader.range(&meter, None, None, cosmwasm_std::Order::Ascending)?;

        let mut leaves: Vec<[u8; 32]> = Vec::new();
        let mut found: Option<(usize, Vec<u8>)> = None;
        for entry in iter {
            let (k, v) = entry?;
            if k.first() == Some(&b'_') {
                continue; // sidecar keys are not consensus state
            }
            if k == key {
                found = Some((leaves.len(), v.clone()));
            }
            let mut h = Sha256::new();
            h.update([0x00u8]);
            h.update(&k);
            h.update(&v);
            leaves.push(h.finalize().into());
        }
        reader.abort();

        let (leaf_index, value) = match found {
            Some(f) => f,
            None => return Ok(None),
        };

        Ok(Some(StateProof {
            state_height,
            key: key.to_vec(),
            value,
            leaf_index: leaf_index as u64,
            siblings: merkle_path(&leaves, leaf_index),
        }))
    }

    /// One-pass state-sync snapshot export: streams all committed
    /// consensus KV (excluding `_` sidecar keys — the same filter
    /// `state_root_over` uses) into length-prefixed `[key_len|key|val_len|val]`
    /// records packed into ≤ `SNAPSHOT_CHUNK_TARGET` chunks with per-chunk
    /// sha256.
    ///
    /// Chunking is a transport detail, not a proof scheme — the flat
    /// sorted-leaf Merkle means the joiner authenticates the WHOLE dump
    /// with one `state_root` recomputation (docs/STATE_SYNC.md §3).
    /// Per-chunk checksums are corruption detection only.
    ///
    /// Height, records and `state_root` all come from the SAME storage
    /// reader, so the export can never disagree with itself if a block
    /// commits mid-request.
    pub fn snapshot_export(&self) -> PulsarResult<SnapshotExport> {
        let meter = GasMeter::infinite();
        let reader = self.storage.reader();

        let height = LAST_BLOCK
            .may_load(&reader, &meter)?
            .map(|b| b.height)
            .unwrap_or(0);

        let iter = reader.range(&meter, None, None, cosmwasm_std::Order::Ascending)?;
        let mut leaves: Vec<[u8; 32]> = Vec::new();
        let mut chunks: Vec<SnapshotChunk> = Vec::new();
        let mut cur: Vec<u8> = Vec::new();

        for entry in iter {
            let (key, value) = entry?;
            if key.first() == Some(&b'_') {
                continue; // sidecar keys are not consensus state
            }
            let mut h = Sha256::new();
            h.update([0x00u8]);
            h.update(&key);
            h.update(&value);
            leaves.push(h.finalize().into());

            let rec_len = 4 + key.len() + 4 + value.len();
            if cur.len() + rec_len > SNAPSHOT_CHUNK_TARGET && !cur.is_empty() {
                chunks.push(SnapshotChunk {
                    checksum: Sha256::digest(&cur).into(),
                    data: std::mem::take(&mut cur),
                });
            }
            cur.extend_from_slice(&(key.len() as u32).to_le_bytes());
            cur.extend_from_slice(&key);
            cur.extend_from_slice(&(value.len() as u32).to_le_bytes());
            cur.extend_from_slice(&value);
        }
        reader.abort();

        if !cur.is_empty() || chunks.is_empty() {
            chunks.push(SnapshotChunk {
                checksum: Sha256::digest(&cur).into(),
                data: cur,
            });
        }

        Ok(SnapshotExport {
            height,
            state_root: merkle_root(&leaves),
            format: SNAPSHOT_FORMAT_V1,
            chunks,
        })
    }

    /// State-sync adopt step (joiner side — docs/STATE_SYNC.md §4).
    ///
    /// `records` is the full decoded chunk stream fetched via
    /// `/layer.statesync.v1.Query/LoadSnapshotChunk`; `expected_root` MUST
    /// be the `state_root` from a BLS-certified `BlockPayload` — never
    /// the snapshot metadata itself. The whole-dump root check runs
    /// BEFORE any write: a mismatch means the dump is doctored and
    /// nothing is imported.
    ///
    /// On success all KV land in ONE storage commit together with
    /// `LAST_BLOCK` at `block` and the `app/state` sidecar, `self.data`
    /// is populated, and the `_state_root` sidecar is recomputed — the
    /// same tail-of-commit invariants `finalize_block` maintains, so a
    /// restart loads cleanly and consensus resumes at `block.height + 1`.
    /// `tip_payload` is the serialized consensus `BlockPayload` for the
    /// block at `block.height` — REQUIRED: `LayerNode` links the executed
    /// tip via `get_block_payload(height)` (`_block_payload:` sidecar is
    /// `_`-prefixed and therefore NOT part of the dump). Without it the
    /// parent-digest chain can never reach the adopted tip and execution
    /// wedges on the first finalized block.
    pub fn snapshot_import(
        &mut self,
        block: BlockInfo,
        records: &[(Vec<u8>, Vec<u8>)],
        expected_root: &[u8; 32],
        tip_payload: &[u8],
    ) -> PulsarResult<()> {
        if !verify_snapshot_root(records, expected_root) {
            return Err(
                StdError::generic_err("snapshot root mismatch — refusing to adopt state").into(),
            );
        }

        let mut writer = self.storage.writer();
        let meter = GasMeter::infinite();
        for (k, v) in records {
            writer.set(&meter, k, v)?;
        }
        LAST_BLOCK.save(&mut writer, &meter, &block)?;
        let key = format!("{}{}", BLOCK_PAYLOAD_KEY_PREFIX, block.height);
        let payload_item: Item<Vec<u8>> = Item::new(&key);
        payload_item.save(&mut writer, &meter, &tip_payload.to_vec())?;
        writer.commit(&meter)?;

        // app/state comes from the dump — if it is missing, the dump was
        // not produced by snapshot_export and we refuse to continue.
        self.load_from_storage().map_err(|e| {
            StdError::generic_err(format!("snapshot import: state load failed: {e}"))
        })?;
        self.store_state_root()?;
        Ok(())
    }

    #[cfg(test)]
    pub fn copy_storage_to_memory(&self) -> layer_storage::MemoryStore {
        layer_storage::MemoryStore::import(&self.storage.reader(), None).unwrap()
    }
}

/// State root over every non-sidecar (`'_'`-prefixed keys excluded) entry of
/// `store`, in ascending key order. Works over a reader (committed state) or
/// a writer (state including its uncommitted writes).
fn state_root_over(store: &dyn ReadonlyStorage) -> PulsarResult<[u8; 32]> {
    let meter = GasMeter::infinite();
    let iter = store.range(&meter, None, None, cosmwasm_std::Order::Ascending)?;
    let mut leaves: Vec<[u8; 32]> = Vec::new();
    for entry in iter {
        let (key, value) = entry?;
        if key.first() == Some(&b'_') {
            continue; // sidecar keys are not consensus state
        }
        let mut h = Sha256::new();
        h.update([0x00u8]);
        h.update(&key);
        h.update(&value);
        leaves.push(h.finalize().into());
    }
    Ok(merkle_root(&leaves))
}

/// Snapshot wire format — bump if the record/chunk layout changes.
pub const SNAPSHOT_FORMAT_V1: u32 = 1;

/// Chunk payloads stay below this; a record is never split across chunks.
pub const SNAPSHOT_CHUNK_TARGET: usize = 8 * 1024 * 1024;

/// One transport chunk of a state-sync snapshot.
#[derive(Clone)]
pub struct SnapshotChunk {
    /// Record stream `[key_len u32le | key | val_len u32le | val]*`, keys
    /// ascending, `_`-prefixed sidecar keys excluded.
    pub data: Vec<u8>,
    /// sha256(data) — corruption detection only; authentication is the
    /// whole-dump `state_root` check on the joiner.
    pub checksum: [u8; 32],
}

/// Exported state-sync snapshot: metadata + transport chunks.
#[derive(Clone)]
pub struct SnapshotExport {
    /// Committed height the snapshot was taken at.
    pub height: u64,
    /// `state_root` recomputed over the exported records (same storage
    /// snapshot as the records themselves).
    pub state_root: [u8; 32],
    /// Always `SNAPSHOT_FORMAT_V1` today.
    pub format: u32,
    /// Transport chunks, in order.
    pub chunks: Vec<SnapshotChunk>,
}

/// Decode one snapshot chunk's records — the joiner side of the wire
/// format. Kept next to the encoder so the layout has exactly one
/// definition.
pub fn decode_snapshot_chunk(data: &[u8]) -> PulsarResult<Vec<(Vec<u8>, Vec<u8>)>> {
    let mut out = Vec::new();
    let mut rest = data;
    while !rest.is_empty() {
        if rest.len() < 4 {
            return Err(StdError::generic_err("snapshot record: truncated key length").into());
        }
        let klen = u32::from_le_bytes(rest[..4].try_into().unwrap()) as usize;
        rest = &rest[4..];
        if rest.len() < klen {
            return Err(StdError::generic_err("snapshot record: truncated key").into());
        }
        let key = rest[..klen].to_vec();
        rest = &rest[klen..];
        if rest.len() < 4 {
            return Err(StdError::generic_err("snapshot record: truncated value length").into());
        }
        let vlen = u32::from_le_bytes(rest[..4].try_into().unwrap()) as usize;
        rest = &rest[4..];
        if rest.len() < vlen {
            return Err(StdError::generic_err("snapshot record: truncated value").into());
        }
        out.push((key, rest[..vlen].to_vec()));
        rest = &rest[vlen..];
    }
    Ok(out)
}

/// The joiner's whole-dump authentication: sort records by key (canonical
/// leaf order), recompute the Merkle root, compare to the `state_root`
/// extracted from a BLS-certified `BlockPayload`. One check authenticates
/// every byte — any missing, extra, or tampered record shifts the root.
pub fn verify_snapshot_root(records: &[(Vec<u8>, Vec<u8>)], expected_root: &[u8; 32]) -> bool {
    let mut sorted: Vec<&(Vec<u8>, Vec<u8>)> = records.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let leaves: Vec<[u8; 32]> = sorted
        .iter()
        .map(|(k, v)| {
            let mut h = Sha256::new();
            h.update([0x00u8]);
            h.update(k);
            h.update(v);
            h.finalize().into()
        })
        .collect();
    merkle_root(&leaves) == *expected_root
}

/// Compute the root of a domain-separated binary Merkle tree over sorted
/// leaf hashes. Odd nodes promote unchanged; a single leaf is its own root;
/// the empty tree hashes to sha256 of the empty input.
///
/// Shared by `compute_state_root` (full-state root) and the proof-serving
/// path in grpc.rs (path extraction over the same leaf list). The contract
/// verifier in `contracts/light-client/src/merkle.rs` mirrors this exactly.
pub(crate) fn merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return Sha256::digest([]).into();
    }
    let mut level: Vec<[u8; 32]> = leaves.to_vec();
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

/// Build the sibling path for the leaf at `index` in the same tree
/// `merkle_root` constructs. Returns one entry per level, bottom-up:
/// `Some(sibling)` means hash `sha256(0x01 || cur || sibling)` (or the
/// mirrored order when the index bit is odd); `None` means the node
/// promotes unchanged to the next level (odd node count — no sibling).
pub(crate) fn merkle_path(leaves: &[[u8; 32]], index: usize) -> Vec<Option<[u8; 32]>> {
    let mut siblings = Vec::new();
    let mut level: Vec<[u8; 32]> = leaves.to_vec();
    let mut idx = index;
    while level.len() > 1 {
        let sibling = if idx % 2 == 0 { idx + 1 } else { idx - 1 };
        if sibling < level.len() {
            siblings.push(Some(level[sibling]));
        } else {
            // Odd node count — this node promotes unchanged (no sibling).
            siblings.push(None);
        }
        // build next level
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

#[cfg(test)]
mod tests {
    use super::*;

    use bytes::Bytes;
    use cosmwasm_std::{coin, coins, to_json_binary, Binary, Timestamp};
    use hex_literal::hex;

    use layer_std::api::{TmPubKey, ValidatorUpdate};
    use layer_std::response::{
        AccountResponse, AuthQueryResponse, BalanceResponse, BankQueryResponse,
    };
    use layer_std::{
        must_id, AccountId, AuthQuery, BankMsg, BankQuery, FeeInfo, Msg, PubKey, SignedTx,
        SigningInfo,
    };
    use layer_storage::MemoryStore;

    use crate::genesis::{BankAccount, WasmParams};
    use crate::sm::AppConfig;

    fn mock_init(genesis: &GenesisState) -> InitChainRequest {
        let app_state = to_json_binary(genesis).unwrap();
        InitChainRequest {
            time: Timestamp::from_nanos(1_673_194_026_078_305_426),
            chain_id: "junoclaw-1".into(),
            consensus_params: Default::default(),
            validators: vec![ValidatorUpdate {
                pub_key: TmPubKey::Ed25519(vec![123u8; 32]),
                power: 1_000_000,
            }],
            app_state,
            initial_height: 1,
        }
    }

    #[test]
    fn transaction_workflow_memory() {
        let storage = MemoryStore::default();
        transaction_workflow(storage);
    }

    #[cfg(feature = "rocksdb")]
    #[test]
    fn transaction_workflow_rocksdb() {
        // always delete, ignore "does not exist" error
        let path = "/tmp/slay3r-test-rocksdb";
        let _ = std::fs::remove_dir_all(path);
        std::fs::create_dir_all(path).unwrap();

        // create rocksdb store and run same tests
        let storage = layer_storage::RockStore::open(path);
        transaction_workflow(storage);
    }

    /// A1 regression: the state root is computed inside the block commit and
    /// must equal a fresh post-commit computation; the payload sidecar lands
    /// in the same commit; neither sidecar affects app_hash.
    #[test]
    fn finalize_block_with_payload_is_atomic_and_consistent() {
        let genesis = GenesisState {
            bank: vec![BankAccount {
                address: "juno1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmdyychx".to_string(),
                balance: coins(1_000, "ujclaw"),
            }],
            wasm: WasmParams {
                gov_account: "juno1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmdyychx".to_string(),
            },
        };
        let block = || Block {
            txs: vec![],
            height: 1,
            time: Timestamp::from_seconds(1690406618),
            proposer_address: vec![1u8; 32],
            last_votes: vec![],
            certificate: None,
        };

        let mut with_payload = App::new(
            MemoryStore::default(),
            StateMachine::new(&AppConfig::new("/tmp/slay3r/atomic_commit_a")),
        );
        with_payload.init(mock_init(&genesis)).unwrap();
        let res_a = with_payload
            .finalize_block_with_payload(block(), Some(b"payload-bytes".to_vec()))
            .unwrap();

        let mut plain = App::new(
            MemoryStore::default(),
            StateMachine::new(&AppConfig::new("/tmp/slay3r/atomic_commit_b")),
        );
        plain.init(mock_init(&genesis)).unwrap();
        let res_b = plain.finalize_block(block()).unwrap();

        assert_eq!(
            with_payload.state_root(),
            Some(with_payload.compute_state_root().unwrap()),
            "in-commit state root must match post-commit recomputation"
        );
        assert_eq!(with_payload.get_block_payload(1), Some(b"payload-bytes".to_vec()));
        assert_eq!(plain.get_block_payload(1), None);
        assert_eq!(res_a.app_hash, res_b.app_hash, "payload sidecar must not affect app_hash");
        assert_eq!(with_payload.state_root(), plain.state_root());
    }

    // this emulates the run of a transaction being submitted
    // query account + balances
    // run simulate
    // run check_tx
    // run finalize_block
    // query account + balances for update
    fn transaction_workflow<T: PersistentStorage + 'static>(storage: T) {
        let sender = must_id("juno1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmdyychx");
        let recipient = must_id("juno1y5hl7x8hxl72dc9gu920eaz6l7vhl0luag99fr");
        let denom: &str = "ujclaw";

        let expected_gas = 16_000u64;

        // assert the proper pubkey for the account
        let sender_key = PubKey::Secp256k1(Binary::from(
            hex!("034f04181eeba35391b858633a765c4a0c189697b40d216354d50890d350c70290").as_slice(),
        ));
        assert_eq!(sender, sender_key.account_id().unwrap());

        let genesis = GenesisState {
            bank: vec![BankAccount {
                address: sender.to_string(),
                balance: coins(2_000_000_000, denom),
            }],
            wasm: WasmParams {
                gov_account: sender.to_string(),
            },
        };
        // TODO: remove from App args, build inside (with config)
        let logic = StateMachine::new(&AppConfig::new("/tmp/slay3r/transaction_workflow"));
        let request = mock_init(&genesis);

        // create the app
        let mut app = App::new(storage, logic);
        app.init(request).unwrap();

        // first empty block
        let block = Block {
            txs: vec![],
            height: 1,
            time: Timestamp::from_seconds(1690406618),
            proposer_address: vec![1u8; 32],
            last_votes: vec![],
            certificate: None,
        };
        app.finalize_block(block).unwrap();

        // query the sender account
        assert_balance(&app, &sender, denom, 2_000_000_000);
        assert_balance(&app, &recipient, denom, 0);

        // query the sender account
        let acct = query_account(&app, &sender);
        assert_eq!(
            acct,
            AccountResponse::External {
                address: sender.clone(),
                pubkey: None,
                sequence: 0
            }
        );

        // simulate to calculate gas
        let mut tx = SignedTx {
            msgs: vec![Msg::Bank(BankMsg::Send {
                sender: sender.clone(),
                recipient: recipient.clone(),
                amount: coins(2_000_000, denom),
            })],
            signer: must_id("juno1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmdyychx"),
            signing_info: SigningInfo {
                message_hash: Binary::from(
                    hex!("6d368a4b8436e0b19a2d06069e0b70086ba7c40e91a9d04d31946c10346d79a9")
                        .as_slice(),
                ),
                sequence: 0,
                pubkey: Some(sender_key.clone()),
                signature: Binary::from(b""),
            },
            fee: FeeInfo {
                fee: None,
                gas_limit: 0,
            },
            timeout_height: None,
            raw_tx: Bytes::from("Some text here"),
        };
        let sim = Query::Simulate(Tx::Signed(tx.clone()));
        let sim_res = app.query(sim).unwrap();
        let gas_used = match sim_res {
            QueryResponse::<PulsarError>::Simulate(TxResult {
                gas:
                    GasInfo {
                        gas_used,
                        gas_wanted,
                    },
                ..
            }) => {
                assert_eq!(gas_wanted, layer_std::api::DEFAULT_BLOCK_GAS);
                gas_used
            }
            x => panic!("Expected SimulateResponse, got {:?}", x),
        };
        // check gas range
        println!("gas used: {}", gas_used);
        assert!(gas_used > expected_gas);
        assert!(gas_used < expected_gas + 2000);

        // create proper tx (from cosmjs)
        tx.fee = FeeInfo {
            fee: Some(coin(2500, "ujclaw")),
            gas_limit: 100000,
        };
        tx.signing_info.signature = Binary::from(hex!("e5367dc058d8942bddc453eb1b61119bf71186693d8fd0f1683ff7a1b4666e3b67d32f3ddf52360e365099f72b2417a9d6034883032ad1ac97b48f73e754351c").as_slice());

        // pass via check_tx
        app.check_tx(Tx::Signed(tx.clone())).result.unwrap();

        // execute in finalize_block (next height)
        let block = Block {
            txs: vec![Tx::Signed(tx)],
            height: 2,
            time: Timestamp::from_seconds(1690406620),
            proposer_address: vec![1u8; 32],
            last_votes: vec![],
            certificate: None,
        };
        let block_res = app.finalize_block(block).unwrap();
        assert_eq!(block_res.tx_results.len(), 1);
        let tx_res = &block_res.tx_results[0];
        // TODO: more checks
        assert!(tx_res.is_ok());
        // check gas range
        let gas_used = tx_res.gas.gas_used;
        println!("gas used: {:?}", gas_used);
        assert!(gas_used > expected_gas);
        assert!(gas_used < expected_gas + 2000);

        // check balances updated (note sender deducts 2500 in gas fees)
        assert_balance(&app, &sender, denom, 1_997_997_500);
        assert_balance(&app, &recipient, denom, 2_000_000);

        // check account set
        let acct = query_account(&app, &sender);
        assert_eq!(
            acct,
            AccountResponse::External {
                address: sender,
                pubkey: Some(sender_key),
                sequence: 1
            }
        );
    }

    fn assert_balance<T: PersistentStorage + 'static>(
        app: &App<T>,
        account: &AccountId,
        denom: &str,
        amount: u128,
    ) {
        let result = app
            .query(
                BankQuery::Balance {
                    address: account.clone(),
                    denom: denom.to_string(),
                }
                .into(),
            )
            .unwrap();
        assert_eq!(
            result,
            BankQueryResponse::Balance(BalanceResponse {
                amount: coin(amount, denom)
            })
            .into()
        );
    }

    fn query_account<T: PersistentStorage + 'static>(
        app: &App<T>,
        account: &AccountId,
    ) -> AccountResponse {
        let result = app
            .query(
                AuthQuery::Account {
                    address: account.clone(),
                }
                .into(),
            )
            .unwrap();
        match result {
            QueryResponse::Auth(AuthQueryResponse::Account(res)) => res,
            x => panic!("Exected AccountResponse, got {:?}", x),
        }
    }

    /// timeout_height: a tx is valid through the given height and expired
    /// strictly above it — enforced identically in check_tx and deliver.
    #[test]
    fn timeout_height_is_enforced() {
        let sender = must_id("juno1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmdyychx");
        let recipient = must_id("juno1y5hl7x8hxl72dc9gu920eaz6l7vhl0luag99fr");
        let denom = "ujclaw";

        let genesis = GenesisState {
            bank: vec![BankAccount {
                address: sender.to_string(),
                balance: coins(1_000_000, denom),
            }],
            wasm: WasmParams {
                gov_account: sender.to_string(),
            },
        };
        let logic = StateMachine::new(&AppConfig::new("/tmp/slay3r/timeout_height"));
        let mut app = App::new(MemoryStore::default(), logic);
        app.init(mock_init(&genesis)).unwrap();

        let block = Block {
            txs: vec![],
            height: 1,
            time: Timestamp::from_seconds(1690406618),
            proposer_address: vec![1u8; 32],
            last_votes: vec![],
            certificate: None,
        };
        app.finalize_block(block).unwrap();
        // committed height is now 1

        let mk_tx = |timeout_height: Option<u64>| {
            Tx::Signed(SignedTx {
                msgs: vec![Msg::Bank(BankMsg::Send {
                    sender: sender.clone(),
                    recipient: recipient.clone(),
                    amount: coins(1, denom),
                })],
                signer: sender.clone(),
                signing_info: SigningInfo {
                    message_hash: Binary::from(vec![0u8; 32]),
                    sequence: 0,
                    pubkey: Some(PubKey::Secp256k1(Binary::from(
                        hex!("034f04181eeba35391b858633a765c4a0c189697b40d216354d50890d350c70290")
                            .as_slice(),
                    ))),
                    signature: Binary::from(b""),
                },
                fee: FeeInfo {
                    fee: None,
                    gas_limit: 0,
                },
                timeout_height,
                raw_tx: Bytes::from("x"),
            })
        };

        // expired in check_tx: committed height 1 > timeout_height 0
        let err = app.check_tx(mk_tx(Some(0))).result.unwrap_err();
        assert!(
            err.to_string().contains("expired"),
            "expected TxExpired, got {err}"
        );

        // boundary: timeout_height == committed height is still valid — the
        // tx must pass the expiry check and fail later (empty signature)
        let err = app.check_tx(mk_tx(Some(1))).result.unwrap_err();
        assert!(
            !err.to_string().contains("expired"),
            "tx at timeout_height boundary must not expire, got {err}"
        );

        // deliver path: the same boundary tx expires one height later
        let block = Block {
            txs: vec![mk_tx(Some(1))],
            height: 2,
            time: Timestamp::from_seconds(1690406620),
            proposer_address: vec![1u8; 32],
            last_votes: vec![],
            certificate: None,
        };
        let res = app.finalize_block(block).unwrap();
        assert_eq!(res.tx_results.len(), 1);
        let tx_res = &res.tx_results[0];
        assert!(!tx_res.is_ok(), "expired tx must fail in deliver");
        let err = tx_res.result.as_ref().unwrap_err().to_string();
        assert!(
            err.contains("expired"),
            "deliver must surface TxExpired, got {err}"
        );
    }

    #[test]
    fn test_set_and_get_block_certificate() {
        let storage = MemoryStore::default();
        let logic = StateMachine::new(&AppConfig::new("/tmp/slay3r/cert_test"));
        let mut app = App::new(storage, logic);

        let genesis = GenesisState {
            bank: vec![],
            wasm: WasmParams {
                gov_account: "juno1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmdyychx".to_string(),
            },
        };
        let request = mock_init(&genesis);
        app.init(request).unwrap();

        // Finalize block 1 so the App has a block at height 1
        let block = Block {
            txs: vec![],
            height: 1,
            time: Timestamp::from_seconds(1690406618),
            proposer_address: vec![1u8; 32],
            last_votes: vec![],
            certificate: None,
        };
        app.finalize_block(block).unwrap();

        // No certificate stored yet for block 1
        assert_eq!(app.get_block_certificate(1), None);

        // Store a certificate for block 1 (simulating the Reporter path)
        let fake_cert = vec![0xCA, 0xFE, 0xBA, 0xBE, 0x01, 0x02, 0x03, 0x04];
        app.set_block_certificate(1, fake_cert.clone()).unwrap();

        // Retrieve and verify it matches what was stored
        assert_eq!(app.get_block_certificate(1), Some(fake_cert));

        // Block 2 has no certificate (no Reporter fired for it yet)
        assert_eq!(app.get_block_certificate(2), None);
    }

    #[test]
    fn test_set_and_get_tx_response() {
        let storage = MemoryStore::default();
        let logic = StateMachine::new(&AppConfig::new("/tmp/slay3r/txres_test"));
        let mut app = App::new(storage, logic);

        // Miss before any write.
        assert_eq!(app.get_tx_response("ABCD"), None);

        let encoded = vec![0x08, 0x07, 0x12, 0x04, 0xAA, 0xBB, 0xCC, 0xDD];
        app.set_tx_response("ABCDEF", &encoded).unwrap();
        assert_eq!(app.get_tx_response("ABCDEF"), Some(encoded));

        // Independent hashes miss; overwriting the same key updates in place.
        assert_eq!(app.get_tx_response("1234"), None);
        let updated = vec![0x01, 0x02];
        app.set_tx_response("ABCDEF", &updated).unwrap();
        assert_eq!(app.get_tx_response("ABCDEF"), Some(updated));
    }

    /// State-sync export: chunk round-trip decodes every record, the
    /// embedded root matches a fresh `compute_state_root`, and the
    /// joiner-side root check rejects tampered/dropped records while
    /// tolerating reshuffles (canonical key sort).
    #[test]
    fn snapshot_export_roundtrip_and_tamper_detection() {
        let genesis = GenesisState {
            bank: vec![BankAccount {
                address: "juno1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmdyychx".to_string(),
                balance: coins(1_000, "ujclaw"),
            }],
            wasm: WasmParams {
                gov_account: "juno1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmdyychx".to_string(),
            },
        };
        let mut app = App::new(
            MemoryStore::default(),
            StateMachine::new(&AppConfig::new("/tmp/slay3r/snapshot_export_test")),
        );
        app.init(mock_init(&genesis)).unwrap();

        let exp = app.snapshot_export().unwrap();
        assert_eq!(exp.format, SNAPSHOT_FORMAT_V1);
        assert!(!exp.chunks.is_empty());
        // embedded root equals the committed root
        assert_eq!(exp.state_root, app.compute_state_root().unwrap());
        // per-chunk checksums are real
        for c in &exp.chunks {
            let cksum: [u8; 32] = Sha256::digest(&c.data).into();
            assert_eq!(c.checksum, cksum);
        }

        // decode all records across chunk boundaries
        let mut records = Vec::new();
        for c in &exp.chunks {
            records.extend(decode_snapshot_chunk(&c.data).unwrap());
        }
        assert!(!records.is_empty(), "genesis must produce KV records");
        // no sidecar keys leaked into the dump
        assert!(records.iter().all(|(k, _)| k.first() != Some(&b'_')));

        // whole-dump root check: clean, shuffled, tampered, dropped
        assert!(verify_snapshot_root(&records, &exp.state_root));

        let mut shuffled = records.clone();
        shuffled.reverse();
        assert!(
            verify_snapshot_root(&shuffled, &exp.state_root),
            "canonical sort must tolerate record order"
        );

        let mut tampered = records.clone();
        tampered[0].1.push(0xFF);
        assert!(
            !verify_snapshot_root(&tampered, &exp.state_root),
            "tampered value must shift root"
        );

        let mut dropped = records.clone();
        dropped.pop();
        assert!(
            !verify_snapshot_root(&dropped, &exp.state_root),
            "missing record must shift root"
        );

        // decoder rejects truncation
        let truncated = &exp.chunks[0].data[..exp.chunks[0].data.len() - 1];
        assert!(
            exp.chunks[0].data.is_empty() || decode_snapshot_chunk(truncated).is_err(),
            "truncated chunk must fail decode"
        );
    }

    /// The joiner path: a fresh App adopts a verified snapshot, lands at
    /// the same state_root, and `info()` reflects the imported height —
    /// the same tail-of-commit state a `finalize_block` would leave.
    /// A doctored dump is refused without touching storage.
    #[test]
    fn snapshot_import_adopts_verified_state() {
        let genesis = GenesisState {
            bank: vec![BankAccount {
                address: "juno1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmdyychx".to_string(),
                balance: coins(1_000, "ujclaw"),
            }],
            wasm: WasmParams {
                gov_account: "juno1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmdyychx".to_string(),
            },
        };
        let mut producer = App::new(
            MemoryStore::default(),
            StateMachine::new(&AppConfig::new("/tmp/slay3r/snap_import_producer")),
        );
        producer.init(mock_init(&genesis)).unwrap();
        let exp = producer.snapshot_export().unwrap();

        let records: Vec<(Vec<u8>, Vec<u8>)> = exp
            .chunks
            .iter()
            .flat_map(|c| decode_snapshot_chunk(&c.data).unwrap())
            .collect();

        let block = cosmwasm_std::BlockInfo {
            height: exp.height,
            time: Timestamp::from_seconds(1690406618),
            chain_id: "junoclaw-1".to_string(),
        };

        // joiner: fresh, never inited
        let mut joiner = App::new(
            MemoryStore::default(),
            StateMachine::new(&AppConfig::new("/tmp/slay3r/snap_import_joiner")),
        );
        assert!(joiner.info().is_none(), "joiner must start uninitialized");

        // doctored dump refused: mutate one record's value
        let mut doctored = records.clone();
        doctored[0].1.push(0xAA);
        assert!(joiner
            .snapshot_import(block.clone(), &doctored, &exp.state_root, b"tip-payload")
            .is_err());
        assert!(joiner.info().is_none(), "refused import must not set info");

        // honest dump adopts
        joiner
            .snapshot_import(block.clone(), &records, &exp.state_root, b"tip-payload")
            .unwrap();
        assert_eq!(joiner.info().unwrap().height, exp.height);
        // tip payload sidecar must be restored — LayerNode links the
        // executed tip's last_digest via get_block_payload(height).
        assert_eq!(
            joiner.get_block_payload(exp.height),
            Some(b"tip-payload".to_vec())
        );
        assert_eq!(
            joiner.compute_state_root().unwrap(),
            exp.state_root,
            "joiner must land on the same state root"
        );
        // exported over the joined state reproduces an identical dump
        let re = joiner.snapshot_export().unwrap();
        assert_eq!(re.state_root, exp.state_root);
        assert_eq!(re.height, exp.height);
        assert_eq!(re.chunks.len(), exp.chunks.len());
        for (a, b) in re.chunks.iter().zip(&exp.chunks) {
            assert_eq!(a.data, b.data);
        }
    }
}
