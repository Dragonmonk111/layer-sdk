use cosmwasm_std::{Order, Record};
use slay3r_std::{GasMeter, GasResult, HexEncode};

/// This is the lowest level of the storage, which can be implemented by MemoryStorage
/// or a real on-disk database. It provides ReadAccessors like MeteredStorage,
/// but one method for bulk write, that will commit a new version and return the app hash (stored internally)
pub trait PersistentStorage {
    type Reader<'x>: ReadonlyStorage
    where
        Self: 'x;

    type Writer<'x>: Transaction
    where
        Self: 'x;

    // open a read-only view of the storage. should abort it to free space for write
    fn reader(&self) -> Self::Reader<'_>;

    // open a read-write view of the storage. takes exclusive access to the storage until completed
    // assumes internal rwlock
    fn writer(&self) -> Self::Writer<'_>;

    /// Returns app hash of last commit
    fn app_hash(&self) -> Vec<u8>;

    // TODO: refactor out the sync stuff better...
    fn latest_sequence(&self) -> u64;
    fn current_state<'a>(&'a self) -> Box<dyn Iterator<Item = (Vec<u8>, Vec<u8>)> + 'a>;
    fn changes_since<'a>(&'a self, _sequence: u64) -> Box<dyn Iterator<Item = BatchChanges> + 'a>;
}

#[derive(Debug, PartialEq)]
pub struct BatchChanges {
    pub sequence: u64,
    pub changes: Vec<StateChange>,
}

#[derive(PartialEq)]
pub enum StateChange {
    Write { key: Vec<u8>, value: Vec<u8> },
    Delete { key: Vec<u8> },
}

impl std::fmt::Debug for StateChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Write { key, value } => f
                .debug_struct("Write")
                .field("key", &stringify_or_hex(&key))
                .field("value", &stringify_or_hex(&value))
                .finish(),
            Self::Delete { key } => f
                .debug_struct("Delete")
                .field("key", &stringify_or_hex(&key))
                .finish(),
        }
    }
}

// TODO: move this to standard utils
pub fn stringify_or_hex(input: &[u8]) -> String {
    std::str::from_utf8(input)
        .map_or_else(|_| HexEncode::new(&input).to_string(), |x| x.to_string())
}

/// This is like cosmwasm_std::Storage, but takes GasMeter as extra arg everywhere
pub trait ReadonlyStorage {
    /// Drops this reader or transaction without committing changes
    /// May be needed to free up resources
    fn abort(self);

    fn get(&self, meter: &GasMeter, key: &[u8]) -> GasResult<Option<Vec<u8>>>;

    fn range<'a>(
        &'a self,
        meter: &'a GasMeter,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: Order,
    ) -> GasResult<Box<dyn Iterator<Item = GasResult<Record>> + 'a>>;
}

pub trait Storage: ReadonlyStorage {
    fn set(&mut self, meter: &GasMeter, key: &[u8], value: &[u8]) -> GasResult<()>;

    fn remove(&mut self, meter: &GasMeter, key: &[u8]) -> GasResult<()>;

    fn as_ref(&self) -> &dyn ReadonlyStorage;
}

pub trait Transaction: Storage {
    // This writes all changes to the underlying storage and consumes this wrapper
    fn commit(self, meter: &GasMeter) -> GasResult<()>;

    fn as_mut(&mut self) -> &mut dyn Storage;
}
