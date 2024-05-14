use serde::{de::DeserializeOwned, Serialize};

use cosmwasm_std::{Binary, CodeInfoResponse, ContractInfoResponse, HexBinary};

use cw_orch_core::contract::interface_traits::{ContractInstance, Uploadable};
use cw_orch_core::environment::{Querier, StateInterface, WasmQuerier};
use cw_orch_core::CwEnvError;

use crate::Slay3rTube;

pub struct Slay3rWasm<S: StateInterface> {
    _store: std::marker::PhantomData<S>,
}

impl<S: StateInterface> Querier for Slay3rWasm<S> {
    type Error = slay3r_app::PulsarError;
}

impl<S: StateInterface> WasmQuerier for Slay3rWasm<S> {
    type Chain = Slay3rTube<S>;

    fn code_id_hash(&self, code_id: u64) -> Result<HexBinary, Self::Error> {
        todo!()
    }

    fn contract_info(
        &self,
        address: impl Into<String>,
    ) -> Result<ContractInfoResponse, Self::Error> {
        todo!()
    }

    fn raw_query(
        &self,
        address: impl Into<String>,
        query_keys: Vec<u8>,
    ) -> Result<Vec<u8>, Self::Error> {
        todo!()
    }

    fn smart_query<Q: Serialize, T: DeserializeOwned>(
        &self,
        address: impl Into<String>,
        query_msg: &Q,
    ) -> Result<T, Self::Error> {
        todo!()
    }

    fn code(&self, code_id: u64) -> Result<CodeInfoResponse, Self::Error> {
        todo!()
    }

    fn local_hash<T: Uploadable + ContractInstance<Self::Chain>>(
        &self,
        contract: &T,
    ) -> Result<HexBinary, CwEnvError> {
        todo!()
    }

    fn instantiate2_addr(
        &self,
        code_id: u64,
        creator: impl Into<String>,
        salt: Binary,
    ) -> Result<String, Self::Error> {
        todo!()
    }
}
