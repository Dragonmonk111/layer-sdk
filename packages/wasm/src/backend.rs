use std::mem::transmute;
use thiserror::Error;

use cosmwasm_std::{
    from_slice, to_binary, Binary, BlockInfo, ContractResult, Empty, QueryRequest, SystemError,
    SystemResult,
};
use cosmwasm_vm::{
    Backend, BackendApi, BackendError, BackendResult, GasInfo, Querier as BackendQuerier,
    Storage as BackendStorage,
};

use pulsar_app::{PulsarError, StateMachine};
use pulsar_std::{AccountId, AccountIdError, GasError, GasMeter};
use pulsar_storage::{ReadonlyStorage, Storage};

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
        _gas_limit: u64,
    ) -> BackendResult<cosmwasm_std::SystemResult<cosmwasm_std::ContractResult<cosmwasm_std::Binary>>>
    {
        let start = self.meter.used();
        // TODO: apply gas limit here on top of meter
        let res = self.do_query_raw(request, self.meter);
        let gas_used = self.meter.used() - start;
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
            _ => todo!(),
        },
        QueryRequest::Wasm(_wasm) => todo!(),
        x => Err(SystemError::UnsupportedRequest {
            kind: format!("{:?}", x),
        }
        .into()),
    }
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
            _ => todo!(),
        },
        _ => todo!(),
    }
}

pub struct VmStore {
    storage: &'static mut dyn Storage,
    meter: &'static GasMeter,
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
        _start: Option<&[u8]>,
        _end: Option<&[u8]>,
        _order: cosmwasm_std::Order,
    ) -> BackendResult<u32> {
        todo!()
    }

    fn next(&mut self, _iterator_id: u32) -> BackendResult<Option<cosmwasm_std::Record>> {
        todo!()
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
