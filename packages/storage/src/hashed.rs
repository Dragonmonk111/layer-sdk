use crate::transactions::Op;
use cosmwasm_std::{Order, Record};
use pulsar_std::{GasMeter, GasResult};

/// This is the lowest level of the storage, which can be implemented by MemoryStorage
/// or a real on-disk database. It provides ReadAccessors like MeteredStorage,
/// but one method for bulk write, that will commit a new version and return the app hash (stored internally)
pub trait PersistentStorage {
    fn get(&self, meter: &mut GasMeter, key: &[u8]) -> GasResult<Option<Vec<u8>>>;

    fn range<'a>(
        &'a self,
        meter: &'a mut GasMeter,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: Order,
    ) -> GasResult<Box<dyn Iterator<Item = GasResult<Record>> + 'a>>;

    /// Writes all pending operations in one batch and returns a new app-hash
    fn commit(&mut self, meter: &mut GasMeter, batch: Vec<Op>) -> GasResult<Vec<u8>>;

    /// Returns app hash of last commit
    fn app_hash(&self) -> Vec<u8>;
}
