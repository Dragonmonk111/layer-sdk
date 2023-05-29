use libc::size_t;
use lmdb::{Cursor, Database, Environment, Transaction};
use std::fmt;
use std::path::Path;
use tracing::{debug_span, trace_span};

use cosmwasm_std::{Order, Record};
use pulsar_std::{GasMeter, GasResult, HexEncode};

use crate::{FastHasher, PersistentStorage, PriceList, ReadonlyStorage, Storage};

// 1 GB max... review this later
pub const DEFAULT_DB_SIZE_MB: u64 = 1024;

// TODO: ensure this never shows up in range queries
pub const APP_HASH_KEY: &[u8] = &[255u8];

pub struct LmdbStore {
    env: Environment,
    db: Database,
}

impl LmdbStore {
    pub fn new(path: &str, max_size_mb: impl Into<Option<u64>>) -> LmdbStore {
        let db_path = Path::new(path);
        let max_size = max_size_mb.into().unwrap_or(DEFAULT_DB_SIZE_MB) * 1024 * 1024;
        let renv = Environment::new()
            .set_map_size(max_size as size_t)
            .open(db_path);
        let env = match renv {
            Ok(x) => x,
            Err(lmdb::Error::Other(2)) => panic!(
                "LMDB database directory does not exist. \
                 Please create it first with `mkdir -p {}`",
                path,
            ),
            Err(lmdb::Error::Other(13)) => panic!(
                "Process does not have write-access to LMDB database directory. \
                 Please update with `chmod +rwx {}`",
                path,
            ),
            Err(lmdb::Error::Other(20)) => panic!(
                "Expected LMDB database directory at {} but found a file. \
                 Please provide a path to a writeable directory.",
                path,
            ),
            Err(e) => panic!("Error opening LMDB database: {:?}", e),
        };
        let db = env.open_db(None).unwrap();
        LmdbStore { env, db }
    }
}

impl fmt::Debug for LmdbStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // FIXME: add some path info?
        write!(f, "LmdbStore")
    }
}

fn read_app_hash<T: Transaction>(tx: &T, db: Database) -> Vec<u8> {
    match tx.get(db, &APP_HASH_KEY) {
        Ok(v) => v.to_vec(),
        // initialize it here
        Err(lmdb::Error::NotFound) => vec![0u8; 32],
        Err(e) => panic!("Error reading from LMDB: {:?}", e),
    }
}

fn write_app_hash(tx: &mut lmdb::RwTransaction, db: Database, hash: &[u8]) {
    tx.put(db, &APP_HASH_KEY, &hash, lmdb::WriteFlags::empty())
        .unwrap();
}

impl PersistentStorage for LmdbStore {
    type Reader<'a> = LmdbReader<'a>;

    type Writer<'a> = LmdbWriter<'a>;

    // open a read-only view of the storage. should abort it to free space for write
    fn reader(&self) -> LmdbReader<'_> {
        let tx = self.env.begin_ro_txn().unwrap();
        LmdbReader {
            tx,
            db: self.db,
            price_list: PriceList::default(),
        }
    }

    // open a read-write view of the storage. takes exclusive access to the storage until completed
    // assumes internal rwlock
    fn writer(&self) -> LmdbWriter<'_> {
        let tx = self.env.begin_rw_txn().unwrap();
        LmdbWriter::new(tx, self.db)
    }

    /// Returns app hash of last commit
    fn app_hash(&self) -> Vec<u8> {
        let tx = self.env.begin_ro_txn().unwrap();
        read_app_hash(&tx, self.db)
    }
}

pub struct LmdbReader<'a> {
    tx: lmdb::RoTransaction<'a>,
    db: Database,
    price_list: PriceList,
}

impl ReadonlyStorage for LmdbReader<'_> {
    fn get(&self, meter: &mut GasMeter, key: &[u8]) -> GasResult<Option<Vec<u8>>> {
        let _span = trace_span!("get", key = %HexEncode::new(&key)).entered();
        let val = match self.tx.get(self.db, &key) {
            Ok(v) => Some(v.to_vec()),
            Err(lmdb::Error::NotFound) => None,
            Err(e) => panic!("Error reading from LMDB: {:?}", e),
        };
        self.price_list.charge_read(meter, key, val.as_deref())?;
        Ok(val)
    }

    fn range<'a>(
        &'a self,
        meter: &'a mut GasMeter,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: Order,
    ) -> GasResult<Box<dyn Iterator<Item = GasResult<Record>> + 'a>> {
        let _span = trace_span!("range").entered();
        self.price_list.charge_range(meter)?;
        let mut cursor = self.tx.open_ro_cursor(self.db).unwrap();
        // TODO: handle reverse order
        if !matches!(order, Order::Ascending) {
            panic!("LMDB only supports ascending order");
        }

        let iter = match start {
            Some(s) => cursor.iter_from(s),
            None => cursor.iter(),
        };

        let res = LmdbIterator::new(cursor, iter, end, meter, &self.price_list);
        Ok(Box::new(res))
    }

    // Drops this storage without committing changes
    fn abort(self) {
        self.tx.abort()
    }
}

pub struct LmdbIterator<'a> {
    #[allow(dead_code)]
    cursor: lmdb::RoCursor<'a>,
    iter: lmdb::Iter<'a>,
    end: Option<Vec<u8>>,
    meter: &'a mut GasMeter,
    price_list: &'a PriceList,
}

impl<'a> LmdbIterator<'a> {
    pub fn new(
        cursor: lmdb::RoCursor<'a>,
        iter: lmdb::Iter<'a>,
        end: Option<&[u8]>,
        meter: &'a mut GasMeter,
        price_list: &'a PriceList,
    ) -> Self {
        Self {
            cursor,
            iter,
            end: end.map(|s| s.to_vec()),
            meter,
            price_list,
        }
    }
}

impl Iterator for LmdbIterator<'_> {
    type Item = GasResult<Record>;

    fn next(&mut self) -> Option<Self::Item> {
        let _span = trace_span!("next").entered();
        match self.iter.next() {
            Some((k, v)) => {
                if let Some(end) = &self.end {
                    if k >= end.as_slice() {
                        return None;
                    }
                }
                if let Err(e) = self.price_list.charge_read(self.meter, k, Some(v)) {
                    return Some(Err(e));
                }
                Some(Ok((k.to_vec(), v.to_vec())))
            }
            None => None,
        }
    }
}

pub struct LmdbWriter<'a> {
    hasher: FastHasher,
    tx: lmdb::RwTransaction<'a>,
    db: Database,
    price_list: PriceList,
}

impl<'a> LmdbWriter<'a> {
    pub fn new(tx: lmdb::RwTransaction<'a>, db: Database) -> Self {
        let app_hash = read_app_hash(&tx, db);
        let hasher = FastHasher::new(&app_hash);
        let price_list = PriceList::default();
        LmdbWriter {
            tx,
            db,
            hasher,
            price_list,
        }
    }
}

impl ReadonlyStorage for LmdbWriter<'_> {
    fn get(&self, meter: &mut GasMeter, key: &[u8]) -> GasResult<Option<Vec<u8>>> {
        let _span = trace_span!("get", key = %HexEncode::new(&key)).entered();
        let val = match self.tx.get(self.db, &key) {
            Ok(v) => Some(v.to_vec()),
            Err(lmdb::Error::NotFound) => None,
            Err(e) => panic!("Error reading from LMDB: {:?}", e),
        };
        self.price_list.charge_read(meter, key, val.as_deref())?;
        Ok(val)
    }

    fn range<'a>(
        &'a self,
        meter: &'a mut GasMeter,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: Order,
    ) -> GasResult<Box<dyn Iterator<Item = GasResult<Record>> + 'a>> {
        let _span = trace_span!("range").entered();
        self.price_list.charge_range(meter)?;
        let mut cursor = self.tx.open_ro_cursor(self.db).unwrap();
        // TODO: handle reverse order
        if !matches!(order, Order::Ascending) {
            panic!("LMDB only supports ascending order");
        }

        let iter = match start {
            Some(s) => cursor.iter_from(s),
            None => cursor.iter(),
        };

        let res = LmdbIterator::new(cursor, iter, end, meter, &self.price_list);
        Ok(Box::new(res))
    }

    // Drops this storage without committing changes
    fn abort(self) {
        self.tx.abort()
    }
}

impl Storage for LmdbWriter<'_> {
    fn set(&mut self, meter: &mut GasMeter, key: &[u8], value: &[u8]) -> GasResult<()> {
        let _span =
            trace_span!("set", key = %HexEncode::new(&key), value = %HexEncode::new(&value))
                .entered();
        self.price_list.charge_write(meter, key, value)?;
        self.tx
            .put(self.db, &key, &value, lmdb::WriteFlags::empty())
            .unwrap();
        self.hasher.set(key, value);
        Ok(())
    }

    fn remove(&mut self, meter: &mut GasMeter, key: &[u8]) -> GasResult<()> {
        let _span = trace_span!("remove", key = %HexEncode::new(&key)).entered();
        self.price_list.charge_remove(meter, key)?;
        self.hasher.remove(key);
        match self.tx.del(self.db, &key, None) {
            Ok(_) => Ok(()),
            Err(lmdb::Error::NotFound) => Ok(()),
            Err(e) => panic!("Error deleting from LMDB {}", e),
        }
    }

    fn as_ref(&self) -> &dyn ReadonlyStorage {
        self
    }
}

impl crate::Transaction for LmdbWriter<'_> {
    // This writes all changes to the underlying storage and consumes this wrapper
    fn commit(mut self, _meter: &mut GasMeter) -> GasResult<()> {
        let _span = debug_span!("commit", db = "lmdb",).entered();
        let app_hash = self.hasher.hash();
        write_app_hash(&mut self.tx, self.db, &app_hash);
        self.tx.commit().unwrap();
        Ok(())
    }

    fn as_mut(&mut self) -> &mut dyn Storage {
        self
    }
}
