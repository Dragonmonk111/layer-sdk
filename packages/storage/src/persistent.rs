use crate::{ReadonlyStorage, Storage};

/// This is the lowest level of the storage, which can be implemented by MemoryStorage
/// or a real on-disk database. It provides ReadAccessors like MeteredStorage,
/// but one method for bulk write, that will commit a new version and return the app hash (stored internally)
pub trait PersistentStorage {
    // open a read-only view of the storage. should abort it to free space for write
    fn read<'a>(&'a self) -> Box<dyn ReadonlyStorage + 'a>;

    // open a read-write view of the storage. takes exclusive access to the storage until completed
    // assumes internal rwlock
    fn write<'a>(&'a self) -> Box<dyn Storage + 'a>;

    /// Returns app hash of last commit
    fn app_hash(&self) -> Vec<u8>;
}
