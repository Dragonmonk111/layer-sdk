use std::{collections::BTreeMap, fmt, mem::transmute};
use thiserror::Error;

use cosmwasm_std::{
    from_json, to_json_binary, Binary, BlockInfo, ContractResult, Order, QueryRequest, SystemError,
    SystemResult,
};
use cosmwasm_vm::{
    Backend, BackendApi, BackendError, BackendResult, GasInfo, Querier as BackendQuerier,
    Storage as BackendStorage,
};

use layer_std::{
    response::CodeInfo,
    root::{CustomRootMsg, CustomRootQuery},
    AccountId, AccountIdError, GasError, GasMeter,
};
use layer_storage::{ReadonlyStorage, Storage};

use crate::{PulsarError, StateMachine};

use super::cache::{sdk_gas_to_wasmer, wasmer_gas_to_sdk};

pub const GAS_COST_CANONICAL_ADDRESS: u64 = 40;
pub const GAS_COST_HUMAN_ADDRESS: u64 = 30;
pub const GAS_COST_VALIDATE_ADDRESS: u64 = 20;

pub type CustomQuery = CustomRootQuery;
pub type CustomMsg = CustomRootMsg;

/// Construct a Backend with raw pointers to stack-local data.
///
/// # Safety
///
/// The returned Backend must not outlive any of: `sm`, `contract_storage`,
/// `query_storage`, `meter`. The caller in cache.rs guarantees this by
/// consuming the Backend within the same stack frame — it is passed to
/// `get_instance`, the instance is used, and then recycled before the
/// function returns.
pub(crate) unsafe fn make_backend(
    sm: &StateMachine,
    contract_storage: &mut dyn Storage,
    query_storage: &dyn ReadonlyStorage,
    meter: &GasMeter,
    block: &BlockInfo,
) -> Backend<VmApi, VmStore, VmQuerier> {
    // transmute erases the non-'static lifetime on the dyn trait fat pointer.
    // This is sound because make_backend's safety contract requires all pointees
    // to outlive the returned Backend (see doc comment above).
    let storage_ptr: *mut (dyn Storage + 'static) = transmute(contract_storage as *mut dyn Storage);
    let query_ptr: *const (dyn ReadonlyStorage + 'static) = transmute(query_storage as *const dyn ReadonlyStorage);
    let storage = VmStore {
        storage: storage_ptr,
        meter: meter as *const GasMeter,
        iterators: BTreeMap::new(),
    };
    let querier = VmQuerier {
        sm: sm as *const StateMachine,
        storage: query_ptr,
        meter: meter as *const GasMeter,
        block: block.clone(),
    };

    Backend {
        api: VmApi,
        storage,
        querier,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct VmApi;

impl BackendApi for VmApi {
    fn addr_validate(&self, input: &str) -> BackendResult<()> {
        let cost = GasInfo::with_cost(GAS_COST_VALIDATE_ADDRESS);
        let res = AccountId::parse_string(input)
            .map(|_| ())
            .map_err(account_error_to_backend);
        (res, cost)
    }

    fn addr_canonicalize(&self, human: &str) -> BackendResult<Vec<u8>> {
        let cost = GasInfo::with_cost(GAS_COST_CANONICAL_ADDRESS);
        let res = AccountId::parse_string(human)
            .map(|id| id.to_vec())
            .map_err(account_error_to_backend);
        (res, cost)
    }

    fn addr_humanize(&self, canonical: &[u8]) -> BackendResult<String> {
        let cost = GasInfo::with_cost(GAS_COST_HUMAN_ADDRESS);
        let res = AccountId::new(canonical)
            .map(|id| id.to_string())
            .map_err(account_error_to_backend);
        (res, cost)
    }
}

fn account_error_to_backend(e: AccountIdError) -> BackendError {
    BackendError::UserErr { msg: e.to_string() }
}

pub struct VmQuerier {
    // SAFETY: pointer valid for the duration of the enclosing cache call;
    // same invariant as VmStore above.
    sm: *const StateMachine,
    storage: *const dyn ReadonlyStorage,
    meter: *const GasMeter,
    block: BlockInfo,
}

// SAFETY: VmQuerier is only used within a single-threaded cache call scope.
// The raw pointers point to data that outlives the VmQuerier.
unsafe impl Send for VmQuerier {}

impl BackendQuerier for VmQuerier {
    fn query_raw(
        &self,
        request: &[u8],
        gas_limit: u64,
    ) -> BackendResult<cosmwasm_std::SystemResult<cosmwasm_std::ContractResult<cosmwasm_std::Binary>>>
    {
        let sdk_gas_limit = wasmer_gas_to_sdk(gas_limit);
        // SAFETY: pointer valid for cache call duration (see make_backend docs)
        let sub_limit = sdk_gas_limit < unsafe { (*self.meter).remaining() };
        let (res, gas_used) = if sub_limit {
            let sub_meter = GasMeter::new(sdk_gas_limit);
            let res = self.do_query_raw(request, &sub_meter);
            let gas_used = sub_meter.used();
            // SAFETY: pointer valid for cache call duration (see make_backend docs)
            let gas_res = unsafe { (*self.meter).charge(gas_used) };
            if let Err(e) = gas_res {
                return (
                    Err(out_of_gas(e)),
                    GasInfo::with_externally_used(sdk_gas_to_wasmer(gas_used)),
                );
            }
            (res, gas_used)
        } else {
            // SAFETY: pointer valid for cache call duration (see make_backend docs)
            let start = unsafe { (*self.meter).used() };
            let res = unsafe { self.do_query_raw(request, &*self.meter) };
            // SAFETY: pointer valid for cache call duration (see make_backend docs)
            let gas_used = unsafe { (*self.meter).used() } - start;
            (res, gas_used)
        };
        (
            encode_error(res),
            GasInfo::with_externally_used(sdk_gas_to_wasmer(gas_used)),
        )
    }
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum QueryError {
    #[error("{0}")]
    Backend(#[from] BackendError),
    #[error("{0}")]
    System(#[from] SystemError),
    #[error("{0}")]
    Contract(String),
}

impl From<String> for QueryError {
    fn from(s: String) -> Self {
        QueryError::Contract(s)
    }
}

impl From<PulsarError> for QueryError {
    fn from(err: PulsarError) -> Self {
        match err {
            PulsarError::AccountId(acct) => account_error_to_backend(acct).into(),
            PulsarError::Gas(gas) => out_of_gas(gas).into(),
            other => QueryError::Contract(other.to_string()),
        }
    }
}

// to nested result
fn encode_error(
    res: Result<Binary, QueryError>,
) -> Result<SystemResult<ContractResult<Binary>>, BackendError> {
    match res {
        Ok(x) => Ok(SystemResult::Ok(ContractResult::Ok(x))),
        Err(QueryError::Backend(e)) => Err(e),
        Err(QueryError::System(e)) => Ok(SystemResult::Err(e)),
        Err(QueryError::Contract(e)) => Ok(SystemResult::Ok(ContractResult::Err(e))),
    }
}
impl VmQuerier {
    fn do_query_raw(&self, request: &[u8], meter: &GasMeter) -> Result<Binary, QueryError> {
        let cosmos: QueryRequest<CustomQuery> =
            from_json(request).map_err(|e| SystemError::InvalidRequest {
                error: e.to_string(),
                request: Binary::from(request),
            })?;
        let query = cosmwasm_query_to_layer(cosmos)?;
        // SAFETY: pointer valid for cache call duration (see make_backend docs)
        let response = unsafe { (*self.sm).query(&*self.storage, meter, &self.block, query) }?;
        layer_response_to_cosmwasm(response)
    }
}

fn cosmwasm_query_to_layer(
    query: QueryRequest<CustomQuery>,
) -> Result<layer_std::Query, QueryError> {
    match query {
        QueryRequest::Bank(bank) => match bank {
            cosmwasm_std::BankQuery::Balance { address, denom } => {
                let address =
                    AccountId::parse_string(&address).map_err(account_error_to_backend)?;
                Ok(layer_std::BankQuery::Balance { address, denom }.into())
            }
            cosmwasm_std::BankQuery::AllBalances { address } => {
                let address =
                    AccountId::parse_string(&address).map_err(account_error_to_backend)?;
                Ok(layer_std::BankQuery::AllBalances { address }.into())
            }
            cosmwasm_std::BankQuery::Supply { denom } => {
                Ok(layer_std::BankQuery::Supply { denom }.into())
            }
            // TODO: BankQuery::DenomMetadata, AllDenomMetadata
            x => unsupported_request(&x),
        },
        QueryRequest::Wasm(wasm) => match wasm {
            cosmwasm_std::WasmQuery::Smart { contract_addr, msg } => {
                let contract_addr =
                    AccountId::parse_string(&contract_addr).map_err(account_error_to_backend)?;
                Ok(layer_std::WasmQuery::Smart { contract_addr, msg }.into())
            }
            cosmwasm_std::WasmQuery::Raw { contract_addr, key } => {
                let contract_addr =
                    AccountId::parse_string(&contract_addr).map_err(account_error_to_backend)?;
                Ok(layer_std::WasmQuery::Raw { contract_addr, key }.into())
            }
            cosmwasm_std::WasmQuery::ContractInfo { contract_addr } => {
                let contract_addr =
                    AccountId::parse_string(&contract_addr).map_err(account_error_to_backend)?;
                Ok(layer_std::WasmQuery::ContractInfo { contract_addr }.into())
            }
            cosmwasm_std::WasmQuery::CodeInfo { code_id } => Ok(layer_std::WasmQuery::CodeInfo {
                code_id,
                include_wasm: false,
            }
            .into()),
            x => unsupported_request(&x),
        },
        x => unsupported_request(&x),
    }
}

fn unsupported_request<T, U: fmt::Debug>(kind: &U) -> Result<T, QueryError> {
    Err(SystemError::UnsupportedRequest {
        kind: format!("{:?}", kind),
    }
    .into())
}

fn layer_response_to_cosmwasm(
    response: layer_std::response::QueryResponse<PulsarError>,
) -> Result<Binary, QueryError> {
    use layer_std::response::BankQueryResponse;
    use layer_std::response::QueryResponse::*;
    match response {
        Bank(bank) => match bank {
            BankQueryResponse::AllBalances(balances) => {
                let res = cosmwasm_std::AllBalanceResponse::new(balances.amount);
                Ok(to_json_binary(&res).unwrap())
            }
            BankQueryResponse::Balance(balance) => {
                let res = cosmwasm_std::BalanceResponse::new(balance.amount);
                Ok(to_json_binary(&res).unwrap())
            }
            BankQueryResponse::Supply(supply) => {
                let res = cosmwasm_std::SupplyResponse::new(supply.amount);
                Ok(to_json_binary(&res).unwrap())
            }
            x => unsupported_response(&x),
        },
        Wasm(wasm) => match wasm {
            layer_std::response::WasmQueryResponse::Smart(data) => Ok(data),
            layer_std::response::WasmQueryResponse::Raw(value) => Ok(value),
            layer_std::response::WasmQueryResponse::ContractInfo(info) => {
                let res = cosmwasm_std::ContractInfoResponse::new(
                    info.code_id,
                    info.creator.into(),
                    info.admin.map(|a| a.into()),
                    info.pinned,
                    info.ibc_port,
                );
                Ok(to_json_binary(&res).unwrap())
            }
            layer_std::response::WasmQueryResponse::CodeInfo(info) => {
                let CodeInfo {
                    code_id,
                    creator,
                    checksum,
                    pinned: _,
                } = info.code_info;
                let cs_bytes: [u8; 32] = checksum.as_slice().try_into().map_err(|_| {
                    SystemError::InvalidResponse {
                        error: "Invalid checksum length".to_string(),
                        response: Binary::from(b""),
                    }
                })?;
                let res = cosmwasm_std::CodeInfoResponse::new(
                    code_id,
                    cosmwasm_std::Addr::unchecked(creator.to_string()),
                    cosmwasm_std::Checksum::from(cs_bytes),
                );
                Ok(to_json_binary(&res).unwrap())
            }
            x => unsupported_response(&x),
        },
        x => unsupported_response(&x),
    }
}

fn unsupported_response<T, U: fmt::Debug>(kind: &U) -> Result<T, QueryError> {
    Err(SystemError::InvalidResponse {
        error: format!("Unknown Response {:?}", kind),
        response: Binary::from(b""),
    }
    .into())
}

pub struct VmStore {
    // SAFETY: pointer valid for the duration of the enclosing cache call;
    // never stored beyond the Backend lifetime. The caller in cache.rs
    // guarantees this by consuming the Backend within the same stack frame
    // (passed to get_instance, recycled before returning).
    storage: *mut dyn Storage,
    meter: *const GasMeter,
    iterators: BTreeMap<u32, Iter>,
}

// SAFETY: VmStore is only used within a single-threaded cache call scope.
// The raw pointers point to data that outlives the VmStore.
unsafe impl Send for VmStore {}

pub(crate) fn out_of_gas(err: GasError) -> BackendError {
    match err {
        GasError::OutOfGas { .. } => BackendError::OutOfGas {},
    }
}

impl BackendStorage for VmStore {
    fn get(&self, key: &[u8]) -> BackendResult<Option<Vec<u8>>> {
        // SAFETY: pointer valid for cache call duration (see make_backend docs)
        let pre = unsafe { (*self.meter).used() };
        let val = unsafe { (*self.storage).get(&*self.meter, key) }.map_err(out_of_gas);
        // SAFETY: pointer valid for cache call duration (see make_backend docs)
        let used = unsafe { (*self.meter).used() } - pre;
        (val, GasInfo::with_externally_used(used))
    }

    fn scan(
        &mut self,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: cosmwasm_std::Order,
    ) -> BackendResult<u32> {
        // TODO: figure out gas here
        let gas_info = GasInfo::free();
        let iter = Iter {
            start: start.map(|x| x.to_vec()),
            end: end.map(|x| x.to_vec()),
            order,
            done: false,
        };
        let idx = (self.iterators.len() + 1) as u32;
        self.iterators.insert(idx, iter);
        (Ok(idx), gas_info)
    }

    fn next(&mut self, iterator_id: u32) -> BackendResult<Option<cosmwasm_std::Record>> {
        // get the iterator and ensure it is live
        let iter = match self.iterators.get_mut(&iterator_id) {
            Some(i) => i,
            None => {
                return (
                    Err(BackendError::iterator_does_not_exist(iterator_id)),
                    GasInfo::free(),
                )
            }
        };
        if iter.done {
            return (Ok(None), GasInfo::free());
        }

        // SAFETY: pointer valid for cache call duration (see make_backend docs)
        let start_gas = unsafe { (*self.meter).used() };

        // read the next value
        let start = iter.start.as_deref();
        let end = iter.end.as_deref();
        let mut ptr = match unsafe { (*self.storage).as_ref() }
            // SAFETY: pointer valid for cache call duration (see make_backend docs)
            .range(unsafe { &*self.meter }, start, end, iter.order)
        {
            Ok(x) => x,
            Err(e) => {
                // SAFETY: pointer valid for cache call duration (see make_backend docs)
                let used = unsafe { (*self.meter).used() } - start_gas;
                let gas = GasInfo::with_externally_used(used);
                return (Err(out_of_gas(e)), gas);
            }
        };
        let record = match ptr.next() {
            Some(Ok(x)) => Some(x),
            None => None,
            Some(Err(e)) => {
                // SAFETY: pointer valid for cache call duration (see make_backend docs)
                let used = unsafe { (*self.meter).used() } - start_gas;
                let gas = GasInfo::with_externally_used(used);
                return (Err(out_of_gas(e)), gas);
            }
        };
        let (val, gas) = match record {
            Some((k, v)) => {
                match iter.order {
                    Order::Ascending => {
                        // move up the start to right after this value
                        // see: extend_one_byte(limit: &[u8]) in cw_storage_plus
                        let mut start = k.clone();
                        start.push(0);
                        iter.start = Some(start);
                    }
                    Order::Descending => {
                        // move down the end to this value
                        iter.end = Some(k.clone());
                    }
                }

                // and return this one
                // SAFETY: pointer valid for cache call duration (see make_backend docs)
                let used = unsafe { (*self.meter).used() } - start_gas;
                let gas = GasInfo::with_externally_used(used);
                (Some((k, v)), gas)
            }
            None => {
                // we hit the end, record that
                iter.done = true;
                // SAFETY: pointer valid for cache call duration (see make_backend docs)
                let used = unsafe { (*self.meter).used() } - start_gas;
                let gas = GasInfo::with_externally_used(used);
                (None, gas)
            }
        };

        (Ok(val), gas)
    }

    fn set(&mut self, key: &[u8], value: &[u8]) -> BackendResult<()> {
        // SAFETY: pointer valid for cache call duration (see make_backend docs)
        let pre = unsafe { (*self.meter).used() };
        let val = unsafe { (*self.storage).set(&*self.meter, key, value) }.map_err(out_of_gas);
        // SAFETY: pointer valid for cache call duration (see make_backend docs)
        let used = unsafe { (*self.meter).used() } - pre;
        (val, GasInfo::with_externally_used(used))
    }

    fn remove(&mut self, key: &[u8]) -> BackendResult<()> {
        // SAFETY: pointer valid for cache call duration (see make_backend docs)
        let pre = unsafe { (*self.meter).used() };
        let val = unsafe { (*self.storage).remove(&*self.meter, key) }.map_err(out_of_gas);
        // SAFETY: pointer valid for cache call duration (see make_backend docs)
        let used = unsafe { (*self.meter).used() } - pre;
        (val, GasInfo::with_externally_used(used))
    }
}

struct Iter {
    start: Option<Vec<u8>>,
    end: Option<Vec<u8>>,
    order: Order,
    done: bool,
}
