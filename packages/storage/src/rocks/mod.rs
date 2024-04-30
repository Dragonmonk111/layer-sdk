use std::fmt;
use std::path::Path;

use rocksdb::{Options, DB};

pub struct RockStore {
    pub db: DB,
}

// TODO: tune dynamically
const NUM_CPUS: i32 = 8;

// Underscore means it is not appended to the hasher,
// Everything should be namespaced and thus avoid collision
// pub const APP_HASH_KEY: &[u8] = b"_app_hash";

impl RockStore {
    /// opens or creates a store
    pub fn open<P: AsRef<Path>>(path: P) -> RockStore {
        let db_opts = default_db_opts();
        RockStore::open_opts(path, db_opts)
    }

    pub fn open_opts<P: AsRef<Path>>(path: P, opts: Options) -> RockStore {
        let db = DB::open(&opts, path).unwrap();
        RockStore { db }
    }
}

impl fmt::Debug for RockStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // FIXME: add some path info?
        write!(f, "RockStore")
    }
}

fn default_db_opts() -> Options {
    let mut opts = Options::default();
    opts.create_if_missing(true);
    opts.create_missing_column_families(true);
    opts.set_atomic_flush(true);

    opts.increase_parallelism(NUM_CPUS);
    opts.set_allow_mmap_writes(true);
    opts.set_allow_mmap_reads(true);

    opts.set_max_log_file_size(1_000_000);
    opts.set_recycle_log_file_num(5);
    opts.set_keep_log_file_num(5);
    opts.set_log_level(rocksdb::LogLevel::Warn);

    opts
}
