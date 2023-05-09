use parking_lot::RwLock;
use std::ops::{Deref, DerefMut};
use thiserror::Error;

// TODO: make our own custom pulsar-storage package to extend (esp with file system backing, transactions...)
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{BlockInfo, Storage};
use cw_storage_plus::Item;

use pulsar_std::response::QueryResponse;
use pulsar_std::{GasMeter, Query, Tx};
use pulsar_storage::{prefixed, prefixed_read, StorageTransaction};

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
pub struct App {
    // State
    storage: RwLock<Box<dyn Storage>>,

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
    // FIXME: move this into the HashedStorage API
    pub app_hash: Vec<u8>,

    pub chain_id: String,

    pub last_block: BlockInfo,
}

impl App {
    /// Re-create a blockchain from existing stored state.
    /// If this fails with AppLoadError::NoStoredState, then we wait for init to be called.
    /// Otherwise we fail on loading.
    pub fn load_from_storage(
        storage: impl Storage + 'static,
        logic: StateMachine,
    ) -> Result<App, AppLoadError> {
        let app_store = prefixed_read(&storage, NAMESPACE_APP);
        let state = APP_STATE
            .may_load(&app_store)
            .map_err(|e| AppLoadError::InvalidState(e.to_string()))?;
        match state {
            Some(state) => Ok(App {
                storage: RwLock::new(Box::new(storage)),
                logic,
                block: RwLock::new(state.last_block),
                chain_id: state.chain_id,
            }),
            None => Err(AppLoadError::NoStoredState),
        }
    }

    /// Called once upon blockchain startup with genesis info, before anything else is called
    pub fn init(
        mut storage: impl Storage + 'static,
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

        // Set up the state machine here
        let genesis = GenesisState::parse(&request.app_state)?;
        logic.init(&mut storage, &last_block, genesis)?;

        // Store the application data
        let state = AppState {
            // TODO: make real hash in Storage API... later
            app_hash: vec![0u8; 32],
            chain_id,
            last_block,
        };
        let mut app_store = prefixed(&mut storage, NAMESPACE_APP);
        APP_STATE.save(&mut app_store, &state)?;
        // TODO: commit to disk

        // Create the response
        let response = InitChainResponse {
            consensus_params: request.consensus_params,
            validators: request.validators,
            // TODO: make real hash in Storage API... later
            app_hash: vec![0u8; 32].into(),
        };
        // And initialize the application
        let app = App {
            storage: RwLock::new(Box::new(storage)),
            logic,
            block: RwLock::new(state.last_block),
            chain_id: state.chain_id,
        };

        Ok((app, response))
    }

    /// Returns serialized response to the query that can be passed back verbatum
    pub fn query(&self, request: Query) -> PulsarResult<QueryResponse> {
        let lock = self.storage.read();
        let block = self.block.read();
        let mut meter = GasMeter::new(DEFAULT_QUERY_GAS);
        let resp = self
            .logic
            .query(lock.deref().as_ref(), &mut meter, block.deref(), request)?;
        Ok(resp)
    }

    pub fn check_tx(&self, tx: Tx) -> TxResult {
        let lock = self.storage.read();
        let block = self.block.read();
        // temporary cache we will throw away
        let mut store = StorageTransaction::new(lock.deref().as_ref());

        // TODO: we could do things like commit write to underlying store on success...
        self.execute_tx(&mut store, block.deref(), tx)
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

        let lock = self.storage.read();
        let block = BlockInfo {
            height: full_block.height,
            time: full_block.time,
            chain_id: self.chain_id.clone(),
        };

        // TODO: re-review where we commit and where we wrap (add some docs)
        // grab all writes here and commit at the end
        let mut block_store = StorageTransaction::new(lock.deref().as_ref());
        let tx_results: Vec<_> = full_block
            .txs
            .into_iter()
            .map(|tx| {
                let mut tx_store = StorageTransaction::new(&block_store);
                let r = self.execute_tx(&mut tx_store, &block, tx);
                if r.result.is_ok() {
                    tx_store.prepare().commit(&mut block_store);
                }
                r
            })
            .collect();

        // FIXME: add end blocker

        // commit to underlying store
        {
            let ops = block_store.prepare();
            let mut lock = self.storage.write();
            ops.commit(lock.as_mut());
        }

        // update block in cache
        let mut new_lock = self.block.write();
        *new_lock.deref_mut() = block;

        // TODO: make app-hash
        let app_hash = vec![full_block.height as u8; 32];

        Ok(FinalizeBlockResponse {
            events: vec![],
            tx_results,
            validator_updates: vec![],
            consensus_param_updates: None,
            app_hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{TmPubKey, ValidatorUpdate};
    use crate::genesis::BankAccount;
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::{coin, to_binary, MemoryStorage};
    use pulsar_std::response::BankQueryResponse;
    use pulsar_std::{AccountId, BankQuery};

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

        let storage = MemoryStorage::new();
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
