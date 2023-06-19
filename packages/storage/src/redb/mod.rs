use redb::{
    Builder, Database, ReadOnlyTable, ReadTransaction, ReadableTable, Table, TableDefinition,
    WriteTransaction,
};
use std::fmt;
use tracing::{debug_span, trace_span};

use cosmwasm_std::{Order, Record};
use pulsar_std::{GasMeter, GasResult, HexEncode};

use crate::{
    FastHasher, PersistentStorage, PriceList, ReadonlyStorage, Storage, DEFAULT_PERSISTED_PRICES,
};

// 1 GB max... review this later
pub const DEFAULT_DB_SIZE_MB: usize = 1024;

pub struct RedbStore {
    db: Database,
}

impl RedbStore {
    pub fn new(path: &str, max_size_mb: impl Into<Option<usize>>) -> RedbStore {
        let max_size = max_size_mb.into().unwrap_or(DEFAULT_DB_SIZE_MB) * 1024 * 1024;
        let db = Builder::new()
            .set_cache_size(max_size)
            .create(path)
            .unwrap();
        // TODO: create tables here first time
        RedbStore { db }
    }
}

impl fmt::Debug for RedbStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // FIXME: add some path info?
        write!(f, "RedbStore")
    }
}

const APP_HASH: TableDefinition<u8, &[u8]> = TableDefinition::new("app_hash");
const APP_HASH_KEY: u8 = 0;

const APP_DATA: TableDefinition<&[u8], &[u8]> = TableDefinition::new("app_data");

fn read_app_hash(tx: &ReadTransaction) -> Vec<u8> {
    let table: ReadOnlyTable<'_, u8, &[u8]> = tx.open_table(APP_HASH).unwrap();
    let res = match table.get(&APP_HASH_KEY).unwrap() {
        Some(v) => v.value().to_vec(),
        // initialize it here
        None => vec![0u8; 32],
    };
    res
}

fn read_app_hash2(tx: &WriteTransaction) -> Vec<u8> {
    let table = tx.open_table(APP_HASH).unwrap();
    let res = match table.get(&APP_HASH_KEY).unwrap() {
        Some(v) => v.value().to_vec(),
        // initialize it here
        None => vec![0u8; 32],
    };
    res
}

fn write_app_hash(tx: &mut WriteTransaction, hash: &[u8]) {
    let mut table = tx.open_table(APP_HASH).unwrap();
    table.insert(&APP_HASH_KEY, hash).unwrap();
}

impl PersistentStorage for RedbStore {
    type Reader<'a> = RedbReader<'a>;

    type Writer<'a> = RedbWriter<'a>;

    // open a read-only view of the storage. should abort it to free space for write
    fn reader(&self) -> RedbReader<'_> {
        let tx = self.db.begin_read().unwrap();
        RedbReader::new(tx, &self.db)
    }

    // open a read-write view of the storage. takes exclusive access to the storage until completed
    // assumes internal rwlock
    fn writer(&self) -> RedbWriter<'_> {
        let tx = self.db.begin_write().unwrap();
        RedbWriter::new(tx, &self.db)
    }

    /// Returns app hash of last commit
    fn app_hash(&self) -> Vec<u8> {
        let tx = self.db.begin_read().unwrap();
        read_app_hash(&tx)
    }
}

pub struct RedbReader<'a> {
    tx: ReadTransaction<'a>,
    table: ReadOnlyTable<'a, &'static [u8], &'static [u8]>,
    db: &'a Database,
    price_list: PriceList,
}

impl<'a> RedbReader<'a> {
    pub fn new(tx: ReadTransaction<'a>, db: &'a Database) -> Self {
        let table = tx.open_table(APP_DATA).unwrap();
        RedbReader {
            tx,
            db,
            table,
            price_list: DEFAULT_PERSISTED_PRICES,
        }
    }
}

impl ReadonlyStorage for RedbReader<'_> {
    fn get(&self, meter: &GasMeter, key: &[u8]) -> GasResult<Option<Vec<u8>>> {
        let _span = trace_span!("get", key = %HexEncode::new(&key)).entered();
        let val = self.table.get(key).unwrap().map(|v| v.value().to_vec());
        self.price_list.charge_read(meter, key, val.as_deref())?;
        Ok(val)
    }

    fn range<'a>(
        &'a self,
        meter: &'a GasMeter,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: Order,
    ) -> GasResult<Box<dyn Iterator<Item = GasResult<Record>> + 'a>> {
        let _span = trace_span!("range").entered();
        self.price_list.charge_range(meter)?;

        let iter = match (start, end) {
            (Some(s), Some(e)) => self.table.range(s..e),
            (Some(s), None) => self.table.range(s..),
            (None, Some(e)) => self.table.range(..e),
            (None, None) => self.table.range::<&[u8]>(..),
        }
        .unwrap();
        let range = match order {
            Order::Ascending => ByteRange::Ascending(iter),
            Order::Descending => ByteRange::Descending(iter.rev()),
        };
        let res = RedbIterator::new(range, meter, &self.price_list);
        Ok(Box::new(res))
    }

    // Drops this storage without committing changes
    fn abort(self) {
        // TODO: do we need to do anything but drop it?
    }
}

enum ByteRange<'a> {
    Ascending(redb::Range<'a, &'static [u8], &'static [u8]>),
    Descending(std::iter::Rev<redb::Range<'a, &'static [u8], &'static [u8]>>),
}

impl ByteRange<'_> {
    fn next(&mut self) -> Option<(Vec<u8>, Vec<u8>)> {
        let next = match self {
            ByteRange::Ascending(iter) => iter.next(),
            ByteRange::Descending(iter) => iter.next(),
        };
        next.map(|res| {
            let (k, v) = res.unwrap();
            (k.value().to_vec(), v.value().to_vec())
        })
    }
}

pub struct RedbIterator<'a> {
    iter: ByteRange<'a>,
    meter: &'a GasMeter,
    price_list: &'a PriceList,
}

impl<'a> RedbIterator<'a> {
    pub fn new(iter: ByteRange<'a>, meter: &'a GasMeter, price_list: &'a PriceList) -> Self {
        Self {
            iter,
            meter,
            price_list,
        }
    }
}

impl Iterator for RedbIterator<'_> {
    type Item = GasResult<Record>;

    fn next(&mut self) -> Option<Self::Item> {
        let _span = trace_span!("next").entered();
        match self.iter.next() {
            Some((k, v)) => {
                if let Err(e) = self.price_list.charge_read(self.meter, &k, Some(&v)) {
                    return Some(Err(e));
                }
                Some(Ok((k, v)))
            }
            None => None,
        }
    }
}

pub struct RedbWriter<'a> {
    hasher: FastHasher,
    table: Table<'a, 'a, &'static [u8], &'static [u8]>,
    tx: WriteTransaction<'a>,
    db: &'a Database,
    price_list: PriceList,
}

impl<'a> RedbWriter<'a> {
    pub fn new(tx: WriteTransaction<'a>, db: &'a Database) -> Self {
        let app_hash = read_app_hash2(&tx);
        let hasher = FastHasher::new(&app_hash);
        let price_list = DEFAULT_PERSISTED_PRICES;
        let table = tx.open_table(APP_DATA).unwrap();
        RedbWriter {
            tx,
            db,
            table,
            hasher,
            price_list,
        }
    }
}

impl ReadonlyStorage for RedbWriter<'_> {
    fn get(&self, meter: &GasMeter, key: &[u8]) -> GasResult<Option<Vec<u8>>> {
        let _span = trace_span!("get", key = %HexEncode::new(&key)).entered();
        let val = self.table.get(key).unwrap().map(|v| v.value().to_vec());
        self.price_list.charge_read(meter, key, val.as_deref())?;
        Ok(val)
    }

    fn range<'a>(
        &'a self,
        meter: &'a GasMeter,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: Order,
    ) -> GasResult<Box<dyn Iterator<Item = GasResult<Record>> + 'a>> {
        let _span = trace_span!("range").entered();
        self.price_list.charge_range(meter)?;

        let iter = match (start, end) {
            (Some(s), Some(e)) => self.table.range(s..e),
            (Some(s), None) => self.table.range(s..),
            (None, Some(e)) => self.table.range(..e),
            (None, None) => self.table.range::<&[u8]>(..),
        }
        .unwrap();
        let range = match order {
            Order::Ascending => ByteRange::Ascending(iter),
            Order::Descending => ByteRange::Descending(iter.rev()),
        };
        let res = RedbIterator::new(range, meter, &self.price_list);
        Ok(Box::new(res))
    }

    // Drops this storage without committing changes
    fn abort(self) {
        self.tx.abort().unwrap();
    }
}

impl Storage for RedbWriter<'_> {
    fn set(&mut self, meter: &GasMeter, key: &[u8], value: &[u8]) -> GasResult<()> {
        let _span =
            trace_span!("set", key = %HexEncode::new(&key), value = %HexEncode::new(&value))
                .entered();
        self.price_list.charge_write(meter, key, value)?;
        self.table.insert(key, value).unwrap();
        self.hasher.set(key, value);
        Ok(())
    }

    fn remove(&mut self, meter: &GasMeter, key: &[u8]) -> GasResult<()> {
        let _span = trace_span!("remove", key = %HexEncode::new(&key)).entered();
        self.price_list.charge_remove(meter, key)?;
        self.hasher.remove(key);
        self.table.remove(key).unwrap();
        Ok(())
    }

    fn as_ref(&self) -> &dyn ReadonlyStorage {
        self
    }
}

impl crate::Transaction for RedbWriter<'_> {
    // This writes all changes to the underlying storage and consumes this wrapper
    fn commit(mut self, _meter: &GasMeter) -> GasResult<()> {
        let _span = debug_span!("commit", db = "lmdb",).entered();
        let app_hash = self.hasher.hash();
        write_app_hash(&mut self.tx, &app_hash);
        self.tx.commit().unwrap();
        Ok(())
    }

    fn as_mut(&mut self) -> &mut dyn Storage {
        self
    }
}
