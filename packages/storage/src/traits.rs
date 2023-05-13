use cosmwasm_std::{Order, Record};
use pulsar_std::{GasMeter, GasResult};

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

/// This is like cosmwasm_std::Storage, but takes GasMeter as extra arg everywhere
pub trait ReadonlyStorage {
    fn get(&self, meter: &mut GasMeter, key: &[u8]) -> GasResult<Option<Vec<u8>>>;

    fn range<'a>(
        &'a self,
        meter: &'a mut GasMeter,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: Order,
    ) -> GasResult<Box<dyn Iterator<Item = GasResult<Record>> + 'a>>;

    // Drops this storage without committing changes
    fn abort(self) -> ();
}
pub trait Storage: ReadonlyStorage {
    fn set(&mut self, meter: &mut GasMeter, key: &[u8], value: &[u8]) -> GasResult<()>;

    fn remove(&mut self, meter: &mut GasMeter, key: &[u8]) -> GasResult<()>;

    // This writes all changes to the underlying storage and consumes this wrapper
    fn commit(self, meter: &mut GasMeter) -> GasResult<()>;

    // Question: transaction as a method here? or just use generic Transaction type?
}
