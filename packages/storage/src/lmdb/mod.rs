use libc::size_t;
use lmdb::{Cursor, Database, Environment, Transaction};
use std::path::Path;

use crate::{FastHasher, PersistentStorage, ReadonlyStorage, Storage};
use cosmwasm_std::{Order, Record};
use pulsar_std::{GasMeter, GasResult};

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
        let path = Path::new(path);
        let max_size = max_size_mb.into().unwrap_or(DEFAULT_DB_SIZE_MB) * 1024 * 1024;
        let env = Environment::new()
            .set_map_size(max_size as size_t)
            .open(path)
            .unwrap();
        let db = env.open_db(None).unwrap();
        LmdbStore { env, db }
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
    fn reader<'a>(&'a self) -> LmdbReader<'a> {
        let tx = self.env.begin_ro_txn().unwrap();
        LmdbReader { tx, db: self.db }
    }

    // open a read-write view of the storage. takes exclusive access to the storage until completed
    // assumes internal rwlock
    fn writer<'a>(&'a self) -> LmdbWriter<'a> {
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
}

impl ReadonlyStorage for LmdbReader<'_> {
    fn get(&self, _meter: &mut GasMeter, key: &[u8]) -> GasResult<Option<Vec<u8>>> {
        match self.tx.get(self.db, &key) {
            Ok(v) => Ok(Some(v.to_vec())),
            Err(lmdb::Error::NotFound) => Ok(None),
            Err(e) => panic!("Error reading from LMDB: {:?}", e),
        }
    }

    fn range<'a>(
        &'a self,
        _meter: &'a mut GasMeter,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: Order,
    ) -> GasResult<Box<dyn Iterator<Item = GasResult<Record>> + 'a>> {
        let mut cursor = self.tx.open_ro_cursor(self.db).unwrap();
        // TODO: handle reverse order
        if !matches!(order, Order::Ascending) {
            panic!("LMDB only supports ascending order");
        }

        let iter = match start {
            Some(s) => cursor.iter_from(s),
            None => cursor.iter(),
        };

        let res = LmdbIterator {
            cursor,
            iter,
            end: end.map(|s| s.to_vec()),
        };
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
}

impl Iterator for LmdbIterator<'_> {
    type Item = GasResult<Record>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.iter.next() {
            Some((k, v)) => {
                if let Some(end) = &self.end {
                    if k >= end.as_slice() {
                        return None;
                    }
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
}

impl<'a> LmdbWriter<'a> {
    pub fn new(tx: lmdb::RwTransaction<'a>, db: Database) -> Self {
        let app_hash = read_app_hash(&tx, db);
        let hasher = FastHasher::new(&app_hash);
        LmdbWriter { tx, db, hasher }
    }
}

impl ReadonlyStorage for LmdbWriter<'_> {
    fn get(&self, _meter: &mut GasMeter, key: &[u8]) -> GasResult<Option<Vec<u8>>> {
        match self.tx.get(self.db, &key) {
            Ok(v) => Ok(Some(v.to_vec())),
            Err(lmdb::Error::NotFound) => Ok(None),
            Err(e) => panic!("Error reading from LMDB: {:?}", e),
        }
    }

    fn range<'a>(
        &'a self,
        _meter: &'a mut GasMeter,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: Order,
    ) -> GasResult<Box<dyn Iterator<Item = GasResult<Record>> + 'a>> {
        let mut cursor = self.tx.open_ro_cursor(self.db).unwrap();
        // TODO: handle reverse order
        if !matches!(order, Order::Ascending) {
            panic!("LMDB only supports ascending order");
        }

        let iter = match start {
            Some(s) => cursor.iter_from(s),
            None => cursor.iter(),
        };

        let res = LmdbIterator {
            cursor,
            iter,
            end: end.map(|s| s.to_vec()),
        };
        Ok(Box::new(res))
    }

    // Drops this storage without committing changes
    fn abort(self) {
        self.tx.abort()
    }
}

impl Storage for LmdbWriter<'_> {
    fn set(&mut self, _meter: &mut GasMeter, key: &[u8], value: &[u8]) -> GasResult<()> {
        self.tx
            .put(self.db, &key, &value, lmdb::WriteFlags::empty())
            .unwrap();
        self.hasher.set(key, value);
        Ok(())
    }

    fn remove(&mut self, _meter: &mut GasMeter, key: &[u8]) -> GasResult<()> {
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
        let app_hash = self.hasher.hash();
        write_app_hash(&mut self.tx, self.db, &app_hash);
        self.tx.commit().unwrap();
        Ok(())
    }

    fn as_ref(&self) -> &dyn ReadonlyStorage {
        self
    }

    fn as_mut(&mut self) -> &mut dyn Storage {
        self
    }
}
