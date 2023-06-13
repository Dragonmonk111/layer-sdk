use std::{collections::HashMap, fmt, mem::transmute};
use thiserror::Error;

use cosmwasm_std::{
    from_slice, to_binary, Binary, BlockInfo, ContractResult, Empty, Order, QueryRequest,
    SystemError, SystemResult,
};
use cosmwasm_vm::{
    Backend, BackendApi, BackendError, BackendResult, GasInfo, Querier as BackendQuerier,
    Storage as BackendStorage,
};

use pulsar_std::{AccountId, AccountIdError, GasError, GasMeter};
use pulsar_storage::{ReadonlyStorage, Storage};

use crate::{PulsarError, StateMachine};

pub const GAS_COST_CANONICAL_ADDRESS: u64 = 40;
pub const GAS_COST_HUMAN_ADDRESS: u64 = 30;

pub type CustomQuery = Empty;

/// A bunch of unsafe lifetime games here...
/// Only call it where you are sure all usage of this backend and instance is completed before the references
pub(crate) unsafe fn danger_will_robinson(
    sm: &StateMachine,
    contract_storage: &mut dyn Storage,
    query_storage: &dyn ReadonlyStorage,
    meter: &GasMeter,
    block: &BlockInfo,
) -> Backend<VmApi, VmStore, VmQuerier> {
    let storage = VmStore {
        storage: transmute(contract_storage),
        meter: &*(meter as *const GasMeter),
        iterators: HashMap::new(),
    };
    let querier = VmQuerier {
        sm: &*(sm as *const StateMachine),
        storage: transmute(query_storage),
        meter: &*(meter as *const GasMeter),
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
    fn canonical_address(&self, human: &str) -> BackendResult<Vec<u8>> {
        let cost = GasInfo::with_cost(GAS_COST_CANONICAL_ADDRESS);
        let res = AccountId::parse_string(human)
            .map(|id| id.to_vec())
            .map_err(account_error_to_backend);
        (res, cost)
    }

    fn human_address(&self, canonical: &[u8]) -> BackendResult<String> {
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
    sm: &'static StateMachine,
    storage: &'static dyn ReadonlyStorage,
    meter: &'static GasMeter,
    block: BlockInfo,
}

impl BackendQuerier for VmQuerier {
    fn query_raw(
        &self,
        request: &[u8],
        gas_limit: u64,
    ) -> BackendResult<cosmwasm_std::SystemResult<cosmwasm_std::ContractResult<cosmwasm_std::Binary>>>
    {
        let sub_limit = gas_limit < self.meter.remaining();
        let (res, gas_used) = if sub_limit {
            let sub_meter = GasMeter::new(gas_limit);
            let res = self.do_query_raw(request, &sub_meter);
            let gas_used = sub_meter.used();
            let gas_res = self.meter.charge(gas_used);
            if let Err(e) = gas_res {
                return (Err(out_of_gas(e)), GasInfo::with_externally_used(gas_used));
            }
            (res, gas_used)
        } else {
            let start = self.meter.used();
            let res = self.do_query_raw(request, self.meter);
            let gas_used = self.meter.used() - start;
            (res, gas_used)
        };
        (encode_error(res), GasInfo::with_externally_used(gas_used))
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
            from_slice(request).map_err(|e| SystemError::InvalidRequest {
                error: e.to_string(),
                request: Binary::from(request),
            })?;
        let query = cosmwasm_query_to_pulsar(cosmos)?;
        let response = self.sm.query(self.storage, meter, &self.block, query)?;
        pulsar_response_to_cosmwasm(response)
    }
}

fn cosmwasm_query_to_pulsar(
    query: QueryRequest<CustomQuery>,
) -> Result<pulsar_std::Query, QueryError> {
    match query {
        QueryRequest::Bank(bank) => match bank {
            cosmwasm_std::BankQuery::Balance { address, denom } => {
                let address =
                    AccountId::parse_string(&address).map_err(account_error_to_backend)?;
                Ok(pulsar_std::BankQuery::Balance { address, denom }.into())
            }
            cosmwasm_std::BankQuery::AllBalances { address } => {
                let address =
                    AccountId::parse_string(&address).map_err(account_error_to_backend)?;
                Ok(pulsar_std::BankQuery::AllBalances { address }.into())
            }
            x => unsupported_request(&x),
        },
        QueryRequest::Wasm(_wasm) => todo!(),
        x => unsupported_request(&x),
    }
}

fn unsupported_request<T, U: fmt::Debug>(kind: &U) -> Result<T, QueryError> {
    Err(SystemError::UnsupportedRequest {
        kind: format!("{:?}", kind),
    }
    .into())
}

fn pulsar_response_to_cosmwasm(
    response: pulsar_std::response::QueryResponse<PulsarError>,
) -> Result<Binary, QueryError> {
    use pulsar_std::response::BankQueryResponse;
    use pulsar_std::response::QueryResponse::*;
    match response {
        Bank(bank) => match bank {
            BankQueryResponse::AllBalances(balances) => {
                let res = cosmwasm_std::AllBalanceResponse {
                    amount: balances.amount,
                };
                Ok(to_binary(&res).unwrap())
            }
            BankQueryResponse::Balance(balance) => {
                let res = cosmwasm_std::BalanceResponse {
                    amount: balance.amount,
                };
                Ok(to_binary(&res).unwrap())
            }
            x => unsupported_response(&x),
        },
        // Wasm(wasm) => todo!(),
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
    storage: &'static mut dyn Storage,
    meter: &'static GasMeter,
    iterators: HashMap<u32, Iter>,
}

pub(crate) fn out_of_gas(err: GasError) -> BackendError {
    match err {
        GasError::OutOfGas { .. } => BackendError::OutOfGas {},
    }
}

impl BackendStorage for VmStore {
    fn get(&self, key: &[u8]) -> BackendResult<Option<Vec<u8>>> {
        let pre = self.meter.used();
        let val = self.storage.get(self.meter, key).map_err(out_of_gas);
        let used = self.meter.used() - pre;
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

        let start_gas = self.meter.used();

        // read the next value
        let start = iter.start.as_deref();
        let end = iter.end.as_deref();
        let mut ptr = match self
            .storage
            .as_ref()
            .range(self.meter, start, end, iter.order)
        {
            Ok(x) => x,
            Err(e) => {
                let used = self.meter.used() - start_gas;
                let gas = GasInfo::with_externally_used(used);
                return (Err(out_of_gas(e)), gas);
            }
        };
        let record = match ptr.next() {
            Some(Ok(x)) => Some(x),
            None => None,
            Some(Err(e)) => {
                let used = self.meter.used() - start_gas;
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
                let used = self.meter.used() - start_gas;
                let gas = GasInfo::with_externally_used(used);
                (Some((k, v)), gas)
            }
            None => {
                // we hit the end, record that
                iter.done = true;
                let used = self.meter.used() - start_gas;
                let gas = GasInfo::with_externally_used(used);
                (None, gas)
            }
        };

        (Ok(val), gas)
    }

    fn set(&mut self, key: &[u8], value: &[u8]) -> BackendResult<()> {
        let pre = self.meter.used();
        let val = self.storage.set(self.meter, key, value).map_err(out_of_gas);
        let used = self.meter.used() - pre;
        (val, GasInfo::with_externally_used(used))
    }

    fn remove(&mut self, key: &[u8]) -> BackendResult<()> {
        let pre = self.meter.used();
        let val = self.storage.remove(self.meter, key).map_err(out_of_gas);
        let used = self.meter.used() - pre;
        (val, GasInfo::with_externally_used(used))
    }
}

struct Iter {
    start: Option<Vec<u8>>,
    end: Option<Vec<u8>>,
    order: Order,
    done: bool,
}
