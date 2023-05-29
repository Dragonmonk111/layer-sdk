use std::{path::PathBuf, collections::HashSet};

use cosmwasm_vm::{CacheOptions, Size};

const DEFAULT_CACHE_MB: usize = 500;
const DEFAULT_INSTANCE_MB: usize = 32;
const CAPABILITIES: &[&str] = &["iterator"];

fn capabilities() -> HashSet<String> {
    CAPABILITIES.iter().map(|s| s.to_string()).collect()
}


// TODO: make more args?
pub fn init_cache(cache_dir: &str) -> () {
    let cache_options = CacheOptions{ 
        base_dir: PathBuf::from(cache_dir),
        memory_cache_size: Size::mebi(DEFAULT_CACHE_MB),
        instance_memory_limit: Size::mebi(DEFAULT_INSTANCE_MB),
        available_capabilities: capabilities(),
    };
    let _ = cache_options;
    // Cache::new(cache_options).unwrap()
}