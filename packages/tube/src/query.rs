mod bank;
mod node;
mod wasm;

pub use bank::Slay3rBank;
pub use node::Slay3rNode;
pub use wasm::Slay3rWasm;

use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;

use cosmwasm_std::{Addr, BlockInfo, Coin};

use cw_orch_core::environment::EnvironmentInfo;
use cw_orch_core::environment::{
    BankQuerier, DefaultQueriers, EnvironmentQuerier, Querier, QuerierGetter, QueryHandler,
    StateInterface,
};

use crate::Slay3rTube;

impl<S: StateInterface> QueryHandler for Slay3rTube<S> {
    type Error = slay3r_app::PulsarError;

    /// Wait for an amount of blocks.
    fn wait_blocks(&self, amount: u64) -> Result<(), Self::Error> {
        unimplemented!();
    }

    /// Wait for an amount of seconds.
    fn wait_seconds(&self, secs: u64) -> Result<(), Self::Error> {
        unimplemented!();
    }

    /// Wait for next block.
    fn next_block(&self) -> Result<(), Self::Error> {
        unimplemented!();
    }

    /// Return current block info see [`BlockInfo`].
    fn block_info(&self) -> Result<BlockInfo, <Self::Node as Querier>::Error> {
        self.node_querier().latest_block()
    }

    fn balance(
        &self,
        address: impl Into<String>,
        denom: Option<String>,
    ) -> Result<Vec<Coin>, <Self::Bank as Querier>::Error> {
        self.bank_querier().balance(address, denom)
    }

    /// Send a QueryMsg to a contract.
    fn query<Q: Serialize + Debug, T: Serialize + DeserializeOwned>(
        &self,
        query_msg: &Q,
        contract_address: &Addr,
    ) -> Result<T, <Self::Wasm as Querier>::Error> {
        self.wasm_querier().smart_query(contract_address, query_msg)
    }
}

impl<S: StateInterface> DefaultQueriers for Slay3rTube<S> {
    type Bank = Slay3rBank;
    type Wasm = Slay3rWasm;
    type Node = Slay3rNode;
}

impl<S: StateInterface> QuerierGetter<Slay3rBank> for Slay3rTube<S> {
    fn querier(&self) -> Slay3rBank {
        todo!()
    }
}

impl<S: StateInterface> QuerierGetter<Slay3rWasm> for Slay3rTube<S> {
    fn querier(&self) -> Slay3rWasm {
        todo!()
    }
}

impl<S: StateInterface> QuerierGetter<Slay3rNode> for Slay3rTube<S> {
    fn querier(&self) -> Slay3rNode {
        todo!()
    }
}

impl<S: StateInterface> EnvironmentQuerier for Slay3rTube<S> {
    fn env_info(&self) -> EnvironmentInfo {
        todo!()
    }
}
