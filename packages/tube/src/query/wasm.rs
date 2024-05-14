use serde::{de::DeserializeOwned, Serialize};

use cosmwasm_std::{from_json, to_json_binary, Binary, CodeInfoResponse, ContractInfoResponse, HexBinary, StdError};

use cw_orch_core::contract::interface_traits::{ContractInstance, Uploadable};
use cw_orch_core::environment::{Querier, WasmQuerier};
use cw_orch_core::CwEnvError;
use slay3r_std::response::{QueryResponse, WasmQueryResponse};
use slay3r_std::{AccountId, WasmQuery};

use crate::Slay3rTube;

pub struct Slay3rWasm {
    tube: Slay3rTube,
}

impl Slay3rWasm {
    pub fn new(tube: &Slay3rTube) -> Self {
        Self { tube: tube.clone() }
    }
}

impl Querier for Slay3rWasm {
    type Error = slay3r_app::PulsarError;
}

impl WasmQuerier for Slay3rWasm {
    type Chain = Slay3rTube;

    fn code_id_hash(&self, code_id: u64) -> Result<HexBinary, Self::Error> {
        let info = self.code(code_id)?;
        Ok(info.checksum)
    }

    fn contract_info(
        &self,
        address: impl Into<String>,
    ) -> Result<ContractInfoResponse, Self::Error> {
        let app = self.tube.app.borrow();
        let contract_addr = AccountId::parse_string(&address.into())?;
        let query = WasmQuery::ContractInfo { contract_addr };
        let res = app.query(query.into())?;
        match res {
            QueryResponse::Wasm(WasmQueryResponse::ContractInfo(r)) => Ok(r.into()),
            _ => return Err(StdError::generic_err("unexpected response").into()),
        }
    }

    fn raw_query(
        &self,
        address: impl Into<String>,
        query_keys: Vec<u8>,
    ) -> Result<Vec<u8>, Self::Error> {
        let app = self.tube.app.borrow();
        let contract_addr = AccountId::parse_string(&address.into())?;
        let query = WasmQuery::Raw { contract_addr, key: query_keys.into() };
        let res = app.query(query.into())?;
        match res {
            QueryResponse::Wasm(WasmQueryResponse::Raw(r)) => Ok(r.into()),
            _ => return Err(StdError::generic_err("unexpected response").into()),
        }
    }

    fn smart_query<Q: Serialize, T: DeserializeOwned>(
        &self,
        address: impl Into<String>,
        query_msg: &Q,
    ) -> Result<T, Self::Error> {
        let app = self.tube.app.borrow();
        let contract_addr = AccountId::parse_string(&address.into())?;
        let msg = to_json_binary(query_msg)?;
        let query = WasmQuery::Smart { contract_addr, msg };
        let res = app.query(query.into())?;
        match res {
            QueryResponse::Wasm(WasmQueryResponse::Smart(r)) => Ok(from_json(&r)?),
            _ => return Err(StdError::generic_err("unexpected response").into()),
        }
    }

    fn code(&self, code_id: u64) -> Result<CodeInfoResponse, Self::Error> {
        let app = self.tube.app.borrow();
        let query = WasmQuery::CodeInfo { code_id, include_wasm: false };
        let res = app.query(query.into())?;
        let res = match res {
            QueryResponse::Wasm(WasmQueryResponse::CodeInfo(r)) => r.code_info,
            _ => return Err(StdError::generic_err("unexpected response").into()),
        };
        let out = CodeInfoResponse::new(res.code_id, res.creator.to_string(), res.checksum.into());
        Ok(out)
    }

    /// Returns the checksum of the WASM file if the env supports it. Will re-upload every time if not supported.
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
