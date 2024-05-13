use serde::Serialize;
use std::cell::RefCell;
use std::fmt::Debug;
use std::rc::Rc;

use cosmwasm_std::{coin, Addr, Coins};
use cosmwasm_std::{Binary, Coin, Uint128};

use cw_orch_core::contract::{interface_traits::Uploadable, WasmPath};
use cw_orch_core::environment::{
    BankQuerier, BankSetter, ChainInfo, ChainState, DefaultQueriers, NetworkInfo, QueryHandler,
    StateInterface, TxHandler,
};
use cw_orch_core::CwEnvError;

// use abstract_cw_multi_test::AppResponse;

use crate::TestState;

#[derive(Clone)]
pub struct Slay3rTube<S: StateInterface = TestState> {
    /// Address used for the operations.
    // pub sender: Rc<SigningAccount>,
    /// Inner mutable state storage for contract addresses and code-ids
    pub state: Rc<RefCell<S>>,
    // Inner mutable cw-multi-test app backend
    // pub app: Rc<RefCell<OsmosisTestApp>>,
}

impl<S: StateInterface> ChainState for Slay3rTube<S> {
    type Out = Rc<RefCell<S>>;

    fn state(&self) -> Self::Out {
        self.state.clone()
    }
}

impl<S: StateInterface> TxHandler for Slay3rTube<S> {
    type Response = ();

    type Error = slay3r_app::PulsarError;

    type ContractSource = ();

    type Sender = ();

    fn sender(&self) -> Addr {
        self.sender.addr()
    }

    fn set_sender(&mut self, _sender: Self::Sender) {}

    /// Uploads a contract to the chain.
    fn upload<T: Uploadable>(&self, contract_source: &T) -> Result<Self::Response, Self::Error> {
        unimplemented!();
    }

    /// Send a InstantiateMsg to a contract.
    fn instantiate<I: Serialize + Debug>(
        &self,
        code_id: u64,
        init_msg: &I,
        label: Option<&str>,
        admin: Option<&Addr>,
        coins: &[cosmwasm_std::Coin],
    ) -> Result<Self::Response, Self::Error> {
        unimplemented!();
    }

    /// Send a Instantiate2Msg to a contract.
    fn instantiate2<I: Serialize + Debug>(
        &self,
        code_id: u64,
        init_msg: &I,
        label: Option<&str>,
        admin: Option<&Addr>,
        coins: &[cosmwasm_std::Coin],
        salt: Binary,
    ) -> Result<Self::Response, Self::Error> {
        unimplemented!();
    }

    /// Send a ExecMsg to a contract.
    fn execute<E: Serialize + Debug>(
        &self,
        exec_msg: &E,
        coins: &[Coin],
        contract_address: &Addr,
    ) -> Result<Self::Response, Self::Error> {
        unimplemented!();
    }

    /// Send a MigrateMsg to a contract.
    fn migrate<M: Serialize + Debug>(
        &self,
        migrate_msg: &M,
        new_code_id: u64,
        contract_address: &Addr,
    ) -> Result<Self::Response, Self::Error> {
        unimplemented!();
    }

    /// Clones the chain with a different sender.
    /// Usually used to call a contract as a different sender.
    fn call_as(&self, sender: &<Self as TxHandler>::Sender) -> Self {
        let mut chain = self.clone();
        chain.set_sender(sender.clone());
        chain
    }
}

// impl<S: StateInterface> BankSetter for Slay3rTube<S> {
//     type T = crate::query::Slay3rBank;

//     fn set_balance(
//         &mut self,
//         address: impl Into<String>,
//         amount: Vec<Coin>,
//     ) -> Result<(), <Self as TxHandler>::Error> {
//         todo!()
//     }
// }
