use parking_lot::RwLock;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use thiserror::Error;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::BlockInfo;

use pulsar_std::response::QueryResponse;
use pulsar_std::{GasMeter, GasResult, Query, Tx};
use pulsar_storage::{
    prefixed, prefixed_read, Item, PersistentStorage, ReadonlyStorage, ScratchTx, Storage, SubTx,
    Transaction,
};

use crate::api::{
    Block, FinalizeBlockResponse, GasInfo, InitChainRequest, InitChainResponse, TxResponse,
    TxResult,
};
use crate::error::PulsarResult;
use crate::genesis::GenesisState;
use crate::sm::StateMachine;

const DEFAULT_QUERY_GAS: u64 = 500_000;

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
        };
        let mut app_store = prefixed(&mut writer, NAMESPACE_APP);
        APP_STATE.save(&mut app_store, &mut meter, &state)?;

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
        };

        Ok((app, response))
    }

    /// Returns serialized response to the query that can be passed back verbatum
    pub fn query(&self, request: Query) -> PulsarResult<QueryResponse> {
        let reader = self.storage.reader();
        let block = self.block.read();
        let mut meter = GasMeter::new(DEFAULT_QUERY_GAS);
        let resp = self
            .logic
            .query(&reader, &mut meter, block.deref(), request);
        reader.abort();
        resp
    }

    pub fn check_tx(&self, tx: Tx) -> TxResult {
        let reader = self.storage.reader();
        let block = self.block.read();
        // temporary cache we will throw away
        let mut store = ScratchTx::new(&reader);

        let res = self.execute_tx(&mut store, block.deref(), tx);
        reader.abort();
        res
    }

    // TODO: this needs to be cleaned up
    fn execute_tx(&self, storage: &mut dyn Storage, block: &BlockInfo, tx: Tx) -> TxResult {
        // validate the transaction
        let data = match self.logic.validate_tx(storage, block, tx) {
            Ok(x) => x,
            Err(e) => {
                return TxResult {
                    gas: Default::default(),
                    result: Err(e),
                }
            }
        };

        // TODO: if this passes, we should ensure auth (sequence / fee) is written,
        // even if messages fail and are reverted
        let gas_wanted = data.gas_wanted;
        // TODO: cap at some block limit
        let mut meter = GasMeter::new(gas_wanted);

        // execute them all
        let resps: Result<Vec<_>, _> = data
            .msgs
            .into_iter()
            .map(|msg| {
                self.logic
                    .process_msg(storage, &mut meter, &data.signer, block, msg)
            })
            .collect();

        // collect responses (todo: combine multiple data results, not just events...)
        let gas_used = meter.used();
        let result = resps.map(|all| {
            let events = all.into_iter().flat_map(|r| r.events).collect();
            TxResponse { data: None, events }
        });

        TxResult {
            gas: GasInfo {
                gas_used,
                gas_wanted,
            },
            result,
        }
    }

    pub fn finalize_block(&self, full_block: Block) -> PulsarResult<FinalizeBlockResponse> {
        // FIXME: add begin blocker

        // FIXME: use reader here, later writer with ops (just optimization)
        let mut writer = self.storage.writer();
        let block = BlockInfo {
            height: full_block.height,
            time: full_block.time,
            chain_id: self.chain_id.clone(),
        };

        // TODO: re-review where we commit and where we wrap (add some docs)
        // grab all writes here and commit at the end
        let mut block_store = SubTx::new(&mut writer);
        // TODO: is this really what we want to do?
        let tx_results: GasResult<Vec<_>> = full_block
            .txs
            .into_iter()
            .map(|tx| {
                let mut tx_store = SubTx::new(&mut block_store);
                let r = self.execute_tx(&mut tx_store, &block, tx);
                if r.result.is_ok() {
                    // TODO: where does meter come from?
                    let mut meter = GasMeter::infinite();
                    tx_store.commit(&mut meter)?;
                }
                Ok(r)
            })
            .collect();
        let tx_results = tx_results?;

        // FIXME: add end blocker

        // commit to underlying store
        // TODO: where does meter come from?
        let mut meter = GasMeter::infinite();
        block_store.commit(&mut meter)?;

        // update block in cache
        let mut new_lock = self.block.write();
        *new_lock.deref_mut() = block;

        Ok(FinalizeBlockResponse {
            events: vec![],
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
            .query(BankQuery::AllBalances { address: account }.into())
            .unwrap();
        // sort balance, output will be in denom order
        balance.sort_by(|a, b| a.denom.cmp(&b.denom));
        match result {
            QueryResponse::Bank(BankQueryResponse::AllBalances(res)) => {
                assert_eq!(res.amount, balance);
            }
            x => panic!("Exected AllBalancesResponse, got {:?}", x),
        }

        // TODO: pull out storage and re-create this - maybe with custom storage types...
        // let storage = app.storage.into_inner();
        // let app2 = App::load_from_storage(storage, app.logic).unwrap();

        // query the recovered state
    }
}
