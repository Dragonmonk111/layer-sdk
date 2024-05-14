use serde::Serialize;
use slay3r_std::api::{Block, TxResult};
use std::cell::RefCell;
use std::fmt::Debug;
use std::rc::Rc;

use abstract_cw_multi_test::AppResponse;
use cosmwasm_std::{Addr, Binary, BlockInfo, Coin};

use cw_orch_core::contract::{interface_traits::Uploadable, WasmPath};
use cw_orch_core::environment::{BankSetter, ChainState, StateInterface, TxHandler};

use slay3r_app::{App, AppConfig, PulsarError, StateMachine};
use slay3r_storage::MemoryStore;

use crate::OrchRegistry;

#[derive(Clone)]
pub struct Slay3rTube {
    /// Block time in milliseconds.
    pub block_time: u64,
    /// Address used for the operations.
    // pub sender: Rc<SigningAccount>,
    /// Inner mutable state storage for contract addresses and code-ids
    pub state: Rc<RefCell<OrchRegistry>>,
    // Inner mutable app backend
    pub app: Rc<RefCell<App<MemoryStore>>>,
}

fn wrap<T>(val: T) -> Rc<RefCell<T>> {
    Rc::new(RefCell::new(val))
}

impl Slay3rTube {
    pub fn new(chain_id: &str, cache_dir: &str) -> Self {
        let sm = StateMachine::new(&AppConfig::new(cache_dir));
        let app = App::new(MemoryStore::new(), sm);
        Self {
            state: wrap(OrchRegistry::new(chain_id)),
            app: wrap(app),
            block_time: 2000,
        }
    }

    // TODO: init

    pub fn run_block(
        &self,
        tx: Vec<slay3r_std::Tx>,
    ) -> Result<Vec<TxResult<PulsarError>>, PulsarError> {
        let mut app = self.app.borrow_mut();
        let info = app.info().unwrap();
        // TODO: non-empty validators
        let full_block = Block {
            txs: tx,
            height: info.height + 1,
            time: info.time.plus_nanos(self.block_time * 1_000_000), // in ms
            proposer_address: vec![],
            last_votes: vec![],
        };
        let res = app.finalize_block(full_block)?;
        Ok(res.tx_results)
    }

    pub fn block_info(&self) -> BlockInfo {
        let app = self.app.borrow();
        app.info().unwrap().clone()
    }

    // TODO: more init stuff
}

impl ChainState for Slay3rTube {
    type Out = Rc<RefCell<OrchRegistry>>;

    fn state(&self) -> Self::Out {
        self.state.clone()
    }
}

// impl TxHandler for Slay3rTube {
//     type Response = AppResponse;

//     type Error = slay3r_app::PulsarError;

//     type ContractSource = ();

//     type Sender = ();

//     fn sender(&self) -> Addr {
//         todo!()
//     }

//     fn set_sender(&mut self, _sender: Self::Sender) {
//         todo!()
//     }

//     /// Uploads a contract to the chain.
//     fn upload<T: Uploadable>(&self, contract_source: &T) -> Result<Self::Response, Self::Error> {
//         unimplemented!();
//     }

//     /// Send a InstantiateMsg to a contract.
//     fn instantiate<I: Serialize + Debug>(
//         &self,
//         code_id: u64,
//         init_msg: &I,
//         label: Option<&str>,
//         admin: Option<&Addr>,
//         coins: &[cosmwasm_std::Coin],
//     ) -> Result<Self::Response, Self::Error> {
//         unimplemented!();
//     }

//     /// Send a Instantiate2Msg to a contract.
//     fn instantiate2<I: Serialize + Debug>(
//         &self,
//         code_id: u64,
//         init_msg: &I,
//         label: Option<&str>,
//         admin: Option<&Addr>,
//         coins: &[cosmwasm_std::Coin],
//         salt: Binary,
//     ) -> Result<Self::Response, Self::Error> {
//         unimplemented!();
//     }

//     /// Send a ExecMsg to a contract.
//     fn execute<E: Serialize + Debug>(
//         &self,
//         exec_msg: &E,
//         coins: &[Coin],
//         contract_address: &Addr,
//     ) -> Result<Self::Response, Self::Error> {
//         unimplemented!();
//     }

//     /// Send a MigrateMsg to a contract.
//     fn migrate<M: Serialize + Debug>(
//         &self,
//         migrate_msg: &M,
//         new_code_id: u64,
//         contract_address: &Addr,
//     ) -> Result<Self::Response, Self::Error> {
//         unimplemented!();
//     }

//     /// Clones the chain with a different sender.
//     /// Usually used to call a contract as a different sender.
//     fn call_as(&self, sender: &<Self as TxHandler>::Sender) -> Self {
//         let mut chain = self.clone();
//         chain.set_sender(sender.clone());
//         chain
//     }
// }

// impl BankSetter for Slay3rTube {
//     type T = crate::query::Slay3rBank;

//     fn set_balance(
//         &mut self,
//         address: impl Into<String>,
//         amount: Vec<Coin>,
//     ) -> Result<(), <Self as TxHandler>::Error> {
//         todo!()
//     }
// }
