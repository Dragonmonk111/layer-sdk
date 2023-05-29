use std::{collections::HashSet, path::PathBuf};

use cosmwasm_std::{Empty, Env, MessageInfo, Response};
use cosmwasm_vm::{
    call_instantiate, Cache, CacheOptions, Checksum, InstanceOptions, Size, VmError,
};
use pulsar_app::StateMachine;
use pulsar_std::GasMeter;
use pulsar_storage::{MemoryStore, PersistentStorage, Storage};

use crate::backend::{danger_will_robinson, VmApi, VmQuerier, VmStore};

const DEFAULT_CACHE_MB: usize = 500;
const DEFAULT_INSTANCE_MB: usize = 32;
// const CAPABILITIES: &[&str] = &["iterator"];
const CAPABILITIES: &[&str] = &["iterator", "staking", "stargate"];
const PRINT_DEBUG: bool = false;

fn capabilities() -> HashSet<String> {
    CAPABILITIES.iter().map(|s| s.to_string()).collect()
}

pub struct VmCache {
    cache: Cache<VmApi, VmStore, VmQuerier>,
    print_debug: bool,
}

impl VmCache {
    // TODO: make more args?
    pub fn init(cache_dir: &str) -> Self {
        let cache_options = CacheOptions {
            base_dir: PathBuf::from(cache_dir),
            memory_cache_size: Size::mebi(DEFAULT_CACHE_MB),
            instance_memory_limit: Size::mebi(DEFAULT_INSTANCE_MB),
            available_capabilities: capabilities(),
        };
        let cache = unsafe { Cache::new(cache_options).unwrap() };
        VmCache {
            cache,
            print_debug: PRINT_DEBUG,
        }
    }

    pub fn store_code(&mut self, wasm: &[u8]) -> Result<Checksum, VmError> {
        self.cache.save_wasm(wasm)
    }

    pub fn load_code(&mut self, checksum: &Checksum) -> Result<Vec<u8>, VmError> {
        self.cache.load_wasm(checksum)
    }

    pub fn pin(&mut self, checksum: &Checksum) -> Result<(), VmError> {
        self.cache.pin(checksum)
    }

    pub fn unpin(&mut self, checksum: &Checksum) -> Result<(), VmError> {
        self.cache.unpin(checksum)
    }

    // TODO: return gas_info, Response
    #[allow(clippy::too_many_arguments)]
    pub fn instantiate(
        &mut self,
        checksum: &Checksum,
        env: &Env,
        info: &MessageInfo,
        msg: &[u8],
        storage: &mut dyn Storage,
        meter: &GasMeter,
        sm: &StateMachine,
        gas_limit: u64,
    ) -> Result<(), VmError> {
        let options = InstanceOptions {
            gas_limit,
            print_debug: self.print_debug,
        };

        // TODO: create SubTx

        // TODO: sub tx that only holds readable access
        // let sub_tx = SubTx::new(storage);
        let fake = MemoryStore::new();
        let query = fake.reader();

        // This is where we fake all the lifetimes....
        let backend = unsafe { danger_will_robinson(sm, storage, &query, meter) };
        let mut instance = self.cache.get_instance(checksum, backend, options)?;
        let result = call_instantiate(&mut instance, env, info, msg);
        instance.recycle();

        // TODO: proper parsing and return values
        let _: Response<Empty> = result.unwrap().unwrap();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        coin,
        testing::{mock_env, mock_info}, to_vec, Order,
    };
    use pulsar_std::AccountId;
    use pulsar_storage::ReadonlyStorage;

    use super::*;

    // v1.0.1
    const CW20_BASE: &[u8] = include_bytes!("../fixtures/cw20_base.wasm");

    #[test]
    fn can_instatiate() {
        let path = "/tmp/pulsar/test-can-instantiate";
        std::fs::create_dir_all(path).unwrap();

        let mut vm = VmCache::init(path);
        let checksum = vm.store_code(CW20_BASE).unwrap();

        // try to instantiate
        let env = mock_env();
        let sender = AccountId::unchecked("Sillyness");
        let info = mock_info(&sender.to_string(), &[coin(55_000, "upulse")]);
        let meter = GasMeter::infinite();
        let sm = StateMachine::new();
        let store = MemoryStore::new();

        let msg = cw20_base::msg::InstantiateMsg {
            name: "pulsar".to_string(),
            symbol: "PLS".to_string(),
            decimals: 6,
            initial_balances: vec![],
            mint: None,
            marketing: None,
        };
        let msg = to_vec(&msg).unwrap();

        let mut writer = store.writer();
        vm.instantiate(
            &checksum,
            &env,
            &info,
            &msg,
            &mut writer,
            &meter,
            &sm,
            meter.limit(),
        )
        .unwrap();

        // query the state was written
        let num = writer.range(&meter, None, None, Order::Ascending).unwrap().count();
        assert_eq!(num, 2);

    }
}
