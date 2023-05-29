use std::{collections::HashSet, path::PathBuf};

use cosmwasm_std::{Empty, Env, MessageInfo, Response};
use cosmwasm_vm::{
    call_instantiate, Cache, CacheOptions, Checksum, InstanceOptions, Size, VmError,
};
use pulsar_app::App;
use pulsar_std::GasMeter;
use pulsar_storage::{MemoryStore, PersistentStorage, Storage};

use crate::backend::{danger_will_robinson, VmApi, VmQuerier, VmStore};

const DEFAULT_CACHE_MB: usize = 500;
const DEFAULT_INSTANCE_MB: usize = 32;
const CAPABILITIES: &[&str] = &["iterator"];
const PRINT_DEBUG: bool = false;

fn capabilities() -> HashSet<String> {
    CAPABILITIES.iter().map(|s| s.to_string()).collect()
}

pub struct VmCache<T: PersistentStorage + 'static> {
    cache: Cache<VmApi, VmStore, VmQuerier<T>>,
    print_debug: bool,
}

impl<T: PersistentStorage + 'static> VmCache<T> {
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
    pub fn instantiate(
        &mut self,
        checksum: &Checksum,
        env: &Env,
        info: &MessageInfo,
        msg: &[u8],
        storage: &mut dyn Storage,
        meter: &GasMeter,
        app: &App<T>,
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
        let backend = unsafe { danger_will_robinson(app, storage, &query, meter) };
        let mut instance = self.cache.get_instance(checksum, backend, options)?;
        let result = call_instantiate(&mut instance, env, info, msg);
        instance.recycle();

        // TODO: proper parsing and return values
        let _: Response<Empty> = result.unwrap().unwrap();
        Ok(())
    }
}
