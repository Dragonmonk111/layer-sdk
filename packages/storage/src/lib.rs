mod fast_hash;
mod gas;
mod prefixed_storage;
mod traits;
mod transactions;

#[cfg(feature = "memory")]
mod memory;
#[cfg(feature = "memory")]
pub use memory::MemoryStore;

#[cfg(feature = "lmdb")]
mod lmdb;
#[cfg(feature = "lmdb")]
pub use crate::lmdb::LmdbStore;

pub use fast_hash::FastHasher;
pub use gas::{GasStorage, PulsarStorage};
pub use prefixed_storage::{prefixed, prefixed_read, PrefixedStorage, ReadonlyPrefixedStorage};
pub use traits::{PersistentStorage, ReadonlyStorage, Storage};
pub use transactions::{transactional, RepLog, StorageTransaction};
