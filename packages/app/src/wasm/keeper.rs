use cosmwasm_schema::cw_serde;
use cosmwasm_vm::{Checksum, VmError};

use cosmwasm_std::{Addr, Binary, BlockInfo, Env};

use pulsar_std::api::MsgResponse;
use pulsar_std::response::{
    CodeInfoResponse, ContractInfoResponse, QueryResponse, WasmQueryResponse,
};
use pulsar_std::{AccountId, GasMeter, WasmMsg, WasmQuery};
use pulsar_storage::{
    prefixed_read, Map, PlusError, PrefixedStorage, ReadonlyPrefixedStorage, ReadonlyStorage,
    Storage,
};

use super::vm::VmCache;
use crate::error::{PulsarError, PulsarResult};
use crate::sm::StateMachine;
// use crate::wasm::WasmError;

pub const NAMESPACE_WASM: &[u8] = b"wasm";
// const CONTRACT_ATTR: &str = "_contract_addr";

// Contract state is kept in Storage, separate from the contracts themselves
const CONTRACTS: Map<&AccountId, ContractData> = Map::new("contracts");
const CODES: Map<u64, CodeInfo> = Map::new("codes");

/// Contract Data includes information about contract, equivalent of `ContractInfo` in wasmd
/// interface.
#[cw_serde]
pub struct ContractData {
    /// Identifier of stored contract code
    pub code_id: u64,
    /// Address of account who initially instantiated the contract
    pub creator: AccountId,
    /// Optional address of account who can execute migrations
    pub admin: Option<AccountId>,
    /// Metadata passed while contract instantiation
    pub label: String,
    /// Blockchain height in the moment of instantiating the contract
    pub created: u64,
    /// If it is pinned or not
    pub pinned: bool,
    // TODO: add ibc info
    // TODO: reverse lookup on pinned contracts
}

#[cw_serde]
pub struct CodeInfo {
    pub code_id: u64,
    /// The address that initially stored the code
    pub creator: AccountId,
    /// The hash of the Wasm blob
    pub checksum: Binary,
}

#[derive(Debug)]
pub struct Wasm {
    cache: VmCache,
}

#[derive(Debug, Clone)]
pub struct WasmConfig {
    pub cache_dir: String,
}

impl Wasm {
    pub fn new(config: &WasmConfig) -> Self {
        // ensure path exists
        let path = config.cache_dir.as_str();
        std::fs::create_dir_all(path).unwrap();
        Wasm {
            cache: VmCache::init(path),
        }
    }

    pub fn process_msg(
        &self,
        storage: &mut dyn Storage,
        meter: &GasMeter,
        _block: &BlockInfo,
        _sm: &StateMachine,
        signer: &AccountId,
        msg: WasmMsg,
    ) -> PulsarResult<MsgResponse> {
        todo!()
    }

    pub fn query(
        &self,
        storage: &dyn ReadonlyStorage,
        meter: &GasMeter,
        block: &BlockInfo,
        sm: &StateMachine,
        request: WasmQuery,
    ) -> PulsarResult<QueryResponse<PulsarError>> {
        let resp = match request {
            WasmQuery::Smart { contract_addr, msg } => {
                let contract = self.load_contract(storage, meter, &contract_addr)?;
                let code = self.load_code(storage, meter, contract.code_id)?;
                let checksum = Checksum::try_from(code.checksum.as_slice()).unwrap();
                let sub_store = self.read_contract_storage(storage, &contract_addr);
                let env = build_env(block, &contract_addr);
                let (result, gas) =
                    self.cache
                        .query(&checksum.into(), &env, &msg, &sub_store, meter, sm);
                meter.charge(gas)?;
                let result = map_cache_result(result)?;
                WasmQueryResponse::Smart(result)
            }
            WasmQuery::Raw { contract_addr, key } => {
                let sub_store = self.read_contract_storage(storage, &contract_addr);
                let data = sub_store.get(meter, &key)?.unwrap_or_default();
                WasmQueryResponse::Raw(data.into())
            }
            WasmQuery::ContractInfo { contract_addr } => {
                let ContractData {
                    code_id,
                    creator,
                    admin,
                    pinned,
                    ..
                } = self.load_contract(storage, meter, &contract_addr)?;
                let resp = ContractInfoResponse {
                    code_id,
                    creator,
                    admin,
                    pinned,
                    ibc_port: None,
                };
                WasmQueryResponse::ContractInfo(resp)
            }
            WasmQuery::CodeInfo { code_id } => {
                let CodeInfo {
                    code_id,
                    creator,
                    checksum,
                } = self.load_code(storage, meter, code_id)?;
                let resp = CodeInfoResponse {
                    code_id,
                    creator,
                    checksum,
                };
                WasmQueryResponse::CodeInfo(resp)
            }
        };
        Ok(QueryResponse::Wasm(resp))
    }

    fn load_contract(
        &self,
        storage: &dyn ReadonlyStorage,
        meter: &GasMeter,
        address: &AccountId,
    ) -> Result<ContractData, PlusError> {
        CONTRACTS.load(&prefixed_read(storage, NAMESPACE_WASM), meter, address)
    }

    fn load_code(
        &self,
        storage: &dyn ReadonlyStorage,
        meter: &GasMeter,
        id: u64,
    ) -> Result<CodeInfo, PlusError> {
        CODES.load(&prefixed_read(storage, NAMESPACE_WASM), meter, id)
    }

    fn contract_storage<'a>(
        &self,
        storage: &'a mut dyn Storage,
        addr: &AccountId,
    ) -> PrefixedStorage<'a> {
        PrefixedStorage::multilevel(storage, &[NAMESPACE_WASM, addr.as_slice()])
    }

    fn read_contract_storage<'a>(
        &self,
        storage: &'a dyn ReadonlyStorage,
        addr: &AccountId,
    ) -> ReadonlyPrefixedStorage<'a> {
        ReadonlyPrefixedStorage::multilevel(storage, &[NAMESPACE_WASM, addr.as_slice()])
    }
}

fn build_env(block: &BlockInfo, contract: &AccountId) -> Env {
    Env {
        block: block.clone(),
        transaction: None, // review later
        contract: cosmwasm_std::ContractInfo {
            address: Addr::unchecked(contract.to_string()),
        },
    }
}

fn map_cache_result<T>(result: Result<Result<T, String>, VmError>) -> Result<T, PulsarError> {
    match result {
        Ok(Ok(x)) => Ok(x),
        Ok(Err(e)) => panic!("{}", e), // TODO
        Err(e) => panic!("{}", e),     // TODO
    }
}
