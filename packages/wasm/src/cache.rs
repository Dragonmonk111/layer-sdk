use std::{collections::HashSet, path::PathBuf};

use cosmwasm_vm::{Cache, CacheOptions, Size};
use pulsar_storage::PersistentStorage;

use crate::backend::{VmApi, VmQuerier, VmStore};

const DEFAULT_CACHE_MB: usize = 500;
const DEFAULT_INSTANCE_MB: usize = 32;
const CAPABILITIES: &[&str] = &["iterator"];

fn capabilities() -> HashSet<String> {
    CAPABILITIES.iter().map(|s| s.to_string()).collect()
}

pub type VmCache<T> = Cache<VmApi, VmStore, VmQuerier<T>>;

// TODO: make more args?
pub fn init_cache<T: PersistentStorage + 'static>(cache_dir: &str) -> VmCache<T> {
    let cache_options = CacheOptions {
        base_dir: PathBuf::from(cache_dir),
        memory_cache_size: Size::mebi(DEFAULT_CACHE_MB),
        instance_memory_limit: Size::mebi(DEFAULT_INSTANCE_MB),
        available_capabilities: capabilities(),
    };
    unsafe { Cache::new(cache_options).unwrap() }
}
