use parking_lot::RwLock;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use thiserror::Error;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::BlockInfo;

use pulsar_std::response::QueryResponse;
use pulsar_std::{GasMeter, Query, Tx};
use pulsar_storage::{
    atomic, prefixed, prefixed_read, Item, PersistentStorage, ReadonlyStorage, ScratchTx, Storage,
    Transaction,
};

use crate::api::{
    Block, BlockParams, FinalizeBlockResponse, GasInfo, InitChainRequest, InitChainResponse,
    TxResponse, TxResult,
};
use crate::error::{PulsarError, PulsarResult};
use crate::genesis::GenesisState;
use crate::sm::StateMachine;

// FIXME: make this configurable on per-node basis
const DEFAULT_QUERY_GAS: u64 = 500_000;

// these are all consensus critical and must be identical over all nodes
// FIXME: init them from genesis and store them somewhere
const MAX_VALIDATE_GAS: u64 = 200_000;
const MAX_BEGIN_BLOCK_GAS: u64 = 10_000_000;
const MAX_END_BLOCK_GAS: u64 = 10_000_000;

/// This maintains all application global state and is a framework-agnostic entrypoint for the
/// application. It *should* be able to run inside an ABCI app as well as an Avalanche Subnet.
#[allow(dead_code)]
pub struct App<T: PersistentStorage> {
    // State
    storage: Arc<T>,

    // Current Block
    block: RwLock<BlockInfo>,

    // State Machine Logic
    logic: StateMachine,

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

impl<T: PersistentStorage + 'static> App<T> {
    /// Re-create a blockchain from existing stored state.
    /// If this fails with AppLoadError::NoStoredState, then we wait for init to be called.
    /// Otherwise we fail on loading.
    pub fn load_from_storage(storage: T, logic: StateMachine) -> Result<App<T>, AppLoadError> {
        let mut meter = GasMeter::infinite();
        let state = {
            let reader = storage.reader();
            let app_store = prefixed_read(&reader, NAMESPACE_APP);
            APP_STATE
                .may_load(&app_store, &mut meter)
                .map_err(|e| AppLoadError::InvalidState(e.to_string()))?
        };
        match state {
            Some(state) => Ok(App {
                storage: Arc::new(storage),
                logic,
                block: RwLock::new(state.last_block),
                chain_id: state.chain_id,
                params: state.params,
            }),
            None => Err(AppLoadError::NoStoredState),
        }
    }

    /// Called once upon blockchain startup with genesis info, before anything else is called
    pub fn init(
        storage: T,
        logic: StateMachine,
        request: InitChainRequest,
    ) -> PulsarResult<(Self, InitChainResponse)> {
        // Store the state
        let chain_id = request.chain_id.clone();
        let last_block = BlockInfo {
            height: request.initial_height,
            time: request.time,
            chain_id: request.chain_id,
        };

        // start a transaction
        let mut writer = storage.writer();
        let mut meter = GasMeter::infinite();

        // Set up the state machine here
        let genesis = GenesisState::parse(&request.app_state)?;
        logic.init(&mut writer, &mut meter, &last_block, genesis)?;

        // Store the application data
        let state = AppState {
            // TODO: make real hash in Storage API... later
            chain_id,
            last_block,
            params: request.consensus_params.block.clone(),
        };
        {
            // ensure we drop app_store before the commit
            let mut app_store = prefixed(&mut writer, NAMESPACE_APP);
            APP_STATE.save(&mut app_store, &mut meter, &state)?;
        }

        // commit to disk
        writer.commit(&mut meter)?;

        // Create the response
        let response = InitChainResponse {
            consensus_params: request.consensus_params,
            validators: request.validators,
            // TODO: make real hash in Storage API... later
            app_hash: storage.app_hash().into(),
        };
        // And initialize the application
        let app = App {
            storage: Arc::new(storage),
            logic,
            block: RwLock::new(state.last_block),
            chain_id: state.chain_id,
            params: state.params,
        };

        Ok((app, response))
    }

    /// Returns serialized response to the query that can be passed back verbatum
    pub fn query(&self, request: Query) -> PulsarResult<QueryResponse> {
        let reader = self.storage.reader();
        let mut meter = GasMeter::new(DEFAULT_QUERY_GAS);

        // TODO: handle simulate queries
        let block = self.block.read();
        let resp = self
            .logic
            .query(&reader, &mut meter, block.deref(), request);
        drop(block);

        reader.abort();
        resp
    }

    // initialize block gas meter from params
    fn block_gas_meter(&self) -> GasMeter {
        self.params
            .max_gas
            .map(GasMeter::new)
            .unwrap_or_else(GasMeter::infinite)
    }

    pub fn check_tx(&self, tx: Tx) -> TxResult {
        // temporary cache we will throw away
        let reader = self.storage.reader();
        let mut store = ScratchTx::new(&reader);

        // FIXME: only run auth check? or do full tx simulation?
        let mut meter = self.block_gas_meter();
        let block = self.block.read();
        let res = self.execute_tx(&mut store, &mut meter, block.deref(), tx);
        drop(block);

        reader.abort();
        res
    }

    fn execute_tx(
        &self,
        storage: &mut dyn Storage,
        block_meter: &mut GasMeter,
        block: &BlockInfo,
        tx: Tx,
    ) -> TxResult {
        // validate the transaction. if this passes, we commit the auth info (sequence / fee)
        // even if messages fail and are reverted
        let mut val_meter = GasMeter::new(MAX_VALIDATE_GAS);
        let val_res = atomic(storage, &mut val_meter, |store, m| {
            self.logic.validate_tx(store, m, block, tx)
        });
        let data = match val_res {
            Ok(x) => x,
            Err(e) => {
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
        let mut meter = GasMeter::new(gas_wanted);
        if let Err(e) = meter.charge(val_meter.used()) {
            // ignore this out of gas error, aborting anyway and future txs will fail
            let _ = block_meter.charge(val_meter.used());
            return TxResult {
                gas: GasInfo::from_meter(&meter),
                result: Err(e.into()),
            };
        }

        // cap at some block limit - if more requested that fits in the block,
        // abort before trying to run the tx
        if gas_wanted > block_meter.remaining() {
            return TxResult {
                gas: GasInfo::from_meter(&meter),
                result: Err(PulsarError::ExceedsRemainingBlockGas {
                    requested: gas_wanted,
                    remaining: block_meter.remaining(),
                }),
            };
        }

        // execute all messages atomically. if any fail, we don't write any state changes
        // from any of the messages
        let resps: PulsarResult<Vec<_>> = atomic(storage, &mut meter, |store, m| {
            data.msgs
                .into_iter()
                .map(|msg| self.logic.process_msg(store, m, &data.signer, block, msg))
                .collect()
        });
        // we checked block limit above, we shouldn't fail here, so just ignore error
        let _ = block_meter.charge(meter.used());

        // collect responses
        let result = resps.map(|all| {
            // TODO: combine multiple data results, not just events...
            // maybe we need to change TxResponse type to data: Vec<Vec<u8>>? And use separate MsgResponse type
            let events = all.into_iter().flat_map(|r| r.events).collect();
            TxResponse { data: None, events }
        });

        // return result
        let gas = GasInfo::from_meter(&meter);
        TxResult { gas, result }
    }

    pub fn finalize_block(&self, full_block: Block) -> PulsarResult<FinalizeBlockResponse> {
        let mut writer = self.storage.writer();

        // assert we are exactly one block ahead of last known state
        let block = BlockInfo {
            height: full_block.height,
            time: full_block.time,
            chain_id: self.chain_id.clone(),
        };
        let old_block = self.block.read().clone();
        if block.height != old_block.height + 1 {
            return Err(PulsarError::BadBlockHeight {
                got: block.height,
                previous: old_block.height,
            });
        }
        if block.time <= old_block.time {
            return Err(PulsarError::DescendingBlockTime {
                got: old_block.time.seconds(),
                previous: block.time.seconds(),
            });
        }

        // Run begin block logic (not included in block gas)
        let mut begin_meter = GasMeter::new(MAX_BEGIN_BLOCK_GAS);
        let begin_events = self
            .logic
            .begin_block(&mut writer, &mut begin_meter, &full_block)?;

        // Set the block gas meter to limit total gas usage by all txs
        let mut meter = self.block_gas_meter();
        // Execute all transactions within this global limit
        let tx_results: Vec<_> = full_block
            .txs
            .into_iter()
            .map(|tx| {
                // execute tx takes care of atomically committing or aborting auth and msg state writes
                self.execute_tx(&mut writer, &mut meter, &block, tx)
            })
            .collect();

        // Run end block logic (not included in block gas)
        let mut end_meter = GasMeter::new(MAX_END_BLOCK_GAS);
        let end_events = self.logic.end_block(&mut writer, &mut end_meter, &block)?;
        let events = begin_events.into_iter().chain(end_events).collect();

        // Use lock around commit to block any concurrent queries
        let mut new_lock = self.block.write();

        // Commit to underlying store. Use infinite gas meter to ensure we don't fail here
        let mut meter = GasMeter::infinite();
        writer.commit(&mut meter)?;

        // update block in cache
        *new_lock.deref_mut() = block;

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
    use crate::api::{TmPubKey, ValidatorUpdate};
    use crate::genesis::BankAccount;
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::{coin, to_binary};
    use pulsar_std::response::BankQueryResponse;
    use pulsar_std::{AccountId, BankQuery};
    use pulsar_storage::MemoryStore;

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
        let (app, result) = App::init(storage, logic, request.clone()).unwrap();
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
        let app2 = App::load_from_storage(storage, app.logic).unwrap();

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
}
