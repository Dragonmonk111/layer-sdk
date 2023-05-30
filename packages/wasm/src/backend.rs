use std::mem::transmute;

use cosmwasm_vm::{
    Backend, BackendApi, BackendError, BackendResult, GasInfo, Querier as BackendQuerier,
    Storage as BackendStorage,
};

use pulsar_app::StateMachine;
use pulsar_std::{AccountId, AccountIdError, GasError, GasMeter};
use pulsar_storage::{ReadonlyStorage, Storage};

pub const GAS_COST_CANONICAL_ADDRESS: u64 = 40;
pub const GAS_COST_HUMAN_ADDRESS: u64 = 30;

/// A bunch of unsafe lifetime games here...
/// Only call it where you are sure all usage of this backend and instance is completed before the references
pub(crate) unsafe fn danger_will_robinson(
    sm: &StateMachine,
    contract_storage: &mut dyn Storage,
    query_storage: &dyn ReadonlyStorage,
    meter: &GasMeter,
) -> Backend<VmApi, VmStore, VmQuerier> {
    let storage = VmStore {
        storage: transmute(contract_storage),
        meter: &*(meter as *const GasMeter),
    };
    let querier = VmQuerier {
        _sm: &*(sm as *const StateMachine),
        _storage: transmute(query_storage),
        _meter: &*(meter as *const GasMeter),
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
    _sm: &'static StateMachine,
    _storage: &'static dyn ReadonlyStorage,
    _meter: &'static GasMeter,
}

impl BackendQuerier for VmQuerier {
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
