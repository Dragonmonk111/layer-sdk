use thiserror::Error;
use tracing::{
    debug, debug_span,
    field::{debug as dbg, display, Empty},
    info_span,
};

use cosmwasm_schema::cw_serde;
use cosmwasm_std::BlockInfo;

use crate::genesis::GenesisState;
use crate::sm::StateMachine;
use crate::{
    auth::TxData,
    error::{PulsarError, PulsarResult},
};
use pulsar_std::api::{
    Block, BlockParams, FinalizeBlockResponse, GasInfo, InitChainRequest, InitChainResponse,
    TxResponse, TxResult,
};
use pulsar_std::response::QueryResponse;
use pulsar_std::{GasMeter, Query, Rfc3339, Tx};
use pulsar_storage::{
    atomic, prefixed, prefixed_read, Item, PersistentStorage, ReadonlyStorage, ScratchTx, Storage,
    Transaction,
};

// FIXME: make this configurable on per-node basis
const DEFAULT_QUERY_GAS: u64 = 500_000;
const DEFAULT_SIMULATE_GAS: u64 = 10_000_000;

// these are all consensus critical and must be identical over all nodes
// FIXME: init them from genesis and store them somewhere
const MAX_VALIDATE_GAS: u64 = 200_000;
const MAX_BEGIN_BLOCK_GAS: u64 = 10_000_000;
const MAX_END_BLOCK_GAS: u64 = 10_000_000;

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
    storage: T,

    // State Machine Logic
    logic: StateMachine,

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

#[cw_serde]
pub struct AppState {
    pub chain_id: String,

    pub last_block: BlockInfo,

    pub params: BlockParams,
}

// First step to creation
impl<T: PersistentStorage + 'static> App<T> {
    pub fn new(storage: T, logic: StateMachine) -> App<T> {
        App {
            storage,
            logic,
            data: None,
        }
    }
}

// Second creation step
impl<T: PersistentStorage + 'static> App<T> {
    /// Re-create a blockchain from existing stored state.
    /// If this fails with AppLoadError::NoStoredState, then we wait for init to be called.
    /// Otherwise we fail on loading.
    pub fn load_from_storage(&mut self) -> Result<(), AppLoadError> {
        let meter = GasMeter::infinite();
        let state = {
            let reader = self.storage.reader();
            let app_store = prefixed_read(&reader, NAMESPACE_APP);
            APP_STATE
                .may_load(&app_store, &meter)
                .map_err(|e| AppLoadError::InvalidState(e.to_string()))?
        };
        match state {
            Some(state) => {
                debug!(?state, "Loaded state from storage");
                let data = InnerData {
                    block: state.last_block,
                    chain_id: state.chain_id,
                    params: state.params,
                };
                self.data = Some(data);
                Ok(())
            }
            None => Err(AppLoadError::NoStoredState),
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

        // Store the application data
        let state = AppState {
            chain_id,
            last_block,
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
            block: state.last_block,
            chain_id: state.chain_id,
            params: state.params,
        });
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
        let block = self.data.as_ref().map(|d| &d.block);
        debug!(?block, "info");
        block
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
        // temporary cache we will throw away
        let reader = self.storage.reader();
        let mut store = ScratchTx::new(&reader);

        // only run auth check
        let meter = self.block_gas_meter();
        let block = &self.data.as_ref().unwrap().block;
        let res = atomic(&mut store, &meter, |store, m| {
            self.logic.validate_tx(store, m, block, tx)
        });
        reader.abort();

        match res {
            Ok(TxData { gas_wanted, .. }) => {
                let gas_used = gas_wanted;
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

    fn execute_tx(
        &self,
        storage: &mut dyn Storage,
        block_meter: &GasMeter,
        block: &BlockInfo,
        tx: Tx,
    ) -> TxResult<PulsarError> {
        let _span = debug_span!("execute_tx", ?tx, height = block.height).entered();
        // validate the transaction. if this passes, we commit the auth info (sequence / fee)
        // even if messages fail and are reverted
        let val_meter = GasMeter::new(MAX_VALIDATE_GAS);
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
        if let Err(e) = meter.charge(val_meter.used()) {
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
            let data = all
                .iter()
                .map(|r| r.data.clone().unwrap_or_default())
                .collect();
            let events = all.into_iter().map(|r| r.events).collect();
            TxResponse { data, events }
        });
        match &result {
            Ok(r) => debug!(success = ?r, "Tx success"),
            Err(e) => debug!(error = %e, "Tx error"),
        };

        // return result
        let gas = GasInfo::from_meter(&meter);
        TxResult { gas, result }
    }

    pub fn finalize_block(
        &mut self,
        full_block: Block,
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

        // Commit to underlying store. Use infinite gas meter to ensure we don't fail here
        let meter = GasMeter::infinite();
        {
            // ensure we drop app_store before the commit
            let mut app_store = prefixed(&mut writer, NAMESPACE_APP);
            let mut state = APP_STATE.load(&app_store, &meter)?;
            state.last_block = block.clone();
            APP_STATE.save(&mut app_store, &meter, &state)?;
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    use bytes::Bytes;
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::{coin, coins, to_binary, Binary, Timestamp};
    use hex_literal::hex;

    use pulsar_std::api::{TmPubKey, ValidatorUpdate};
    use pulsar_std::response::{
        AccountResponse, AuthQueryResponse, BalanceResponse, BankQueryResponse,
    };
    use pulsar_std::{
        must_id, AccountId, AuthQuery, BankMsg, BankQuery, FeeInfo, Msg, PubKey, SignedTx,
        SigningInfo,
    };
    use pulsar_storage::MemoryStore;

    use crate::genesis::BankAccount;

    fn mock_init(genesis: &GenesisState) -> InitChainRequest {
        let app_state = to_binary(genesis).unwrap();
        let env = mock_env();
        InitChainRequest {
            time: env.block.time,
            chain_id: env.block.chain_id,
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
    fn initialize_and_query_bank() {
        let account = AccountId::unchecked("foobar");
        let mut balance = vec![coin(1_000_000, "upulsar"), coin(2_000_000, "umagic")];
        let genesis = GenesisState {
            bank: vec![BankAccount {
                address: account.to_string(),
                balance: balance.clone(),
            }],
        };

        let storage = MemoryStore::default();
        let logic = StateMachine::new();
        let request = mock_init(&genesis);

        // create the app
        let mut app = App::new(storage, logic);
        let result = app.init(request.clone()).unwrap();
        assert_eq!(result.validators, request.validators);
        assert_eq!(result.consensus_params, request.consensus_params);

        // query the original bank account
        let result = app
            .query(
                BankQuery::AllBalances {
                    address: account.clone(),
                }
                .into(),
            )
            .unwrap();
        // sort balance, output will be in denom order
        balance.sort_by(|a, b| a.denom.cmp(&b.denom));
        match result {
            QueryResponse::Bank(BankQueryResponse::AllBalances(res)) => {
                assert_eq!(res.amount, balance);
            }
            x => panic!("Exected AllBalancesResponse, got {:?}", x),
        }

        // copy data into new storage (MemoryStore::import only meant for testing)
        let storage = MemoryStore::import(&app.storage.reader(), None).unwrap();
        let mut app2 = App::new(storage, app.logic);
        app2.load_from_storage().unwrap();

        // query the recovered state
        let result = app2
            .query(BankQuery::AllBalances { address: account }.into())
            .unwrap();
        match result {
            QueryResponse::Bank(BankQueryResponse::AllBalances(res)) => {
                assert_eq!(res.amount, balance);
            }
            x => panic!("Exected AllBalancesResponse, got {:?}", x),
        }
    }

    #[test]
    fn transaction_workflow_memory() {
        let storage = MemoryStore::default();
        transaction_workflow(storage);
    }

    #[cfg(feature = "lmdb")]
    #[test]
    fn transaction_workflow_lmdb() {
        // always delete, ignore "does not exist" error
        let path = "/tmp/pulsar-test-lmdb";
        let _ = std::fs::remove_dir_all(path);
        std::fs::create_dir_all(path).unwrap();

        // create lmdb store and run same tests
        let storage = pulsar_storage::LmdbStore::new(path, None);
        transaction_workflow(storage);
    }

    // this emulates the run of a transaction being submitted
    // query account + balances
    // run simulate
    // run check_tx
    // run finalize_block
    // query account + balances for update
    fn transaction_workflow<T: PersistentStorage + 'static>(storage: T) {
        let sender = must_id("pulsar1pkptre7fdkl6gfrzlesjjvhxhlc3r4gm6k5p3l");
        let recipient = must_id("pulsar1y5hl7x8hxl72dc9gu920eaz6l7vhl0lu264u06");
        let denom: &str = "upulse";

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
        };
        // TODO: remove from App args, build inside (with config)
        let logic = StateMachine::new();
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
            signer: must_id("pulsar1pkptre7fdkl6gfrzlesjjvhxhlc3r4gm6k5p3l"),
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
                assert_eq!(gas_wanted, pulsar_std::api::DEFAULT_BLOCK_GAS);
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
            fee: Some(coin(2500, "upulse")),
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
}
