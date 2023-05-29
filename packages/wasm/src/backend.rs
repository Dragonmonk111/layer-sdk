use cosmwasm_vm::{
    BackendApi, BackendError, BackendResult, GasInfo, Querier as BackendQuerier,
    Storage as BackendStorage,
};

use pulsar_app::App;
use pulsar_std::{AccountId, AccountIdError, GasMeter};
use pulsar_storage::{PersistentStorage, ReadonlyStorage, Storage};

pub const GAS_COST_CANONICAL_ADDRESS: u64 = 40;
pub const GAS_COST_HUMAN_ADDRESS: u64 = 30;

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

pub struct VmQuerier<T: PersistentStorage + 'static> {
    _app: &'static App<T>,
    _storage: &'static dyn ReadonlyStorage,
}

impl<T: PersistentStorage + 'static> BackendQuerier for VmQuerier<T> {
    fn query_raw(
        &self,
        _request: &[u8],
        _gas_limit: u64,
    ) -> BackendResult<cosmwasm_std::SystemResult<cosmwasm_std::ContractResult<cosmwasm_std::Binary>>>
    {
        todo!()
    }
}

pub struct VmStore {
    _storage: &'static mut dyn Storage,
    _meter: &'static GasMeter,
}

impl BackendStorage for VmStore {
    fn get(&self, key: &[u8]) -> BackendResult<Option<Vec<u8>>> {
        todo!()
    }

    fn scan(
        &mut self,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: cosmwasm_std::Order,
    ) -> BackendResult<u32> {
        todo!()
    }

    fn next(&mut self, iterator_id: u32) -> BackendResult<Option<cosmwasm_std::Record>> {
        todo!()
    }

    fn set(&mut self, key: &[u8], value: &[u8]) -> BackendResult<()> {
        todo!()
    }

    fn remove(&mut self, key: &[u8]) -> BackendResult<()> {
        todo!()
    }
}
