use std::fmt;
use std::path::Path;

use rocksdb::{Direction, IteratorMode, Options, DB};

use cosmwasm_std::{Order, Record};
use slay3r_std::{GasMeter, GasResult, HexEncode};

use crate::{
    FastHasher, PersistentStorage, PriceList, ReadonlyStorage, Storage, Transaction,
    DEFAULT_PERSISTED_PRICES,
};

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
        let db_opts = RockStore::default_db_opts();
        RockStore::open_opts(path, db_opts)
    }

    pub fn open_opts<P: AsRef<Path>>(path: P, opts: Options) -> RockStore {
        let db = DB::open(&opts, path).unwrap();
        RockStore { db }
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
}

impl fmt::Debug for RockStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // FIXME: add some path info?
        write!(f, "RockStore")
    }
}

impl PersistentStorage for RockStore {
    type Reader<'a> = RockReader<'a>;

    type Writer<'a> = RockWriter<'a>;

    // open a read-only view of the storage. should abort it to free space for write
    fn reader(&self) -> RockReader<'_> {
        RockReader {
            db: &self.db,
            price_list: DEFAULT_PERSISTED_PRICES,
        }
    }

    // open a read-write view of the storage. takes exclusive access to the storage until completed
    // assumes internal rwlock
    fn writer(&self) -> RockWriter<'_> {
        RockWriter {
            db: &self.db,
            price_list: DEFAULT_PERSISTED_PRICES,
        }
    }

    /// Returns app hash of last commit
    fn app_hash(&self) -> Vec<u8> {
        todo!();
        // let tx = self.env.begin_ro_txn().unwrap();
        // read_app_hash(&tx, self.db)
    }
}

pub struct RockReader<'a> {
    db: &'a DB,
    price_list: PriceList,
}

impl<'a> ReadonlyStorage for RockReader<'a> {
    fn abort(self) {}

    // NOTE: if we parse it into an object here, we would only need &[u8]
    // and could use get_pinned, avoiding a copy
    fn get(&self, meter: &GasMeter, key: &[u8]) -> GasResult<Option<Vec<u8>>> {
        let val = self.db.get(key).unwrap();
        self.price_list.charge_read(meter, key, val.as_deref())?;
        Ok(val)
    }

    fn range<'b>(
        &'b self,
        meter: &'b GasMeter,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: Order,
    ) -> GasResult<Box<dyn Iterator<Item = GasResult<Record>> + 'b>> {
        todo!()
    }
}

pub struct RockWriter<'a> {
    db: &'a DB,
    price_list: PriceList,
}

impl<'a> ReadonlyStorage for RockWriter<'a> {
    fn abort(self) {}

    fn get(&self, meter: &GasMeter, key: &[u8]) -> GasResult<Option<Vec<u8>>> {
        let val = self.db.get(key).unwrap();
        self.price_list.charge_read(meter, key, val.as_deref())?;
        Ok(val)
    }

    fn range<'b>(
        &'b self,
        meter: &'b GasMeter,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: Order,
    ) -> GasResult<Box<dyn Iterator<Item = GasResult<Record>> + 'b>> {
        todo!()
    }
}

impl<'a> Storage for RockWriter<'a> {
    fn set(&mut self, meter: &GasMeter, key: &[u8], value: &[u8]) -> GasResult<()> {
        // TODO: use transaction or manual batching...
        self.price_list.charge_write(meter, key, value)?;
        self.db.put(key, value).unwrap();
        Ok(())
    }

    fn remove(&mut self, meter: &GasMeter, key: &[u8]) -> GasResult<()> {
        self.price_list.charge_remove(meter, key)?;
        self.db.delete(key).unwrap();
        Ok(())
    }

    fn as_ref(&self) -> &dyn ReadonlyStorage {
        self
    }
}

impl<'a> Transaction for RockWriter<'a> {
    // This writes all changes to the underlying storage and consumes this wrapper
    fn commit(self, meter: &GasMeter) -> GasResult<()> {
        todo!()
    }

    fn as_mut(&mut self) -> &mut dyn Storage {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slay3r_std::GasError;

    #[test]
    fn get_and_set() {
        let storage = RockStore::open("/tmp/foo1");
        let mut store = storage.writer();
        let gas = GasMeter::infinite();
        assert_eq!(store.get(&gas, b"foo").unwrap(), None);
        store.set(&gas, b"foo", b"bar").unwrap();
        assert_eq!(store.get(&gas, b"foo").unwrap(), Some(b"bar".to_vec()));
        assert_eq!(store.get(&gas, b"food").unwrap(), None);
    }

    // #[test]
    // #[should_panic(
    //     expected = "Getting empty values from storage is not well supported at the moment."
    // )]
    // fn set_panics_for_empty() {
    //     let storage = RockStore::new();
    //     let mut store = storage.writer();
    //     let gas = GasMeter::infinite();
    //     store.set(&gas, b"foo", b"").unwrap();
    // }

    #[test]
    fn delete() {
        let storage = RockStore::open("/tmp/foo2");
        let mut store = storage.writer();
        let gas = GasMeter::infinite();
        store.set(&gas, b"foo", b"bar").unwrap();
        store.set(&gas, b"food", b"bank").unwrap();
        store.remove(&gas, b"foo").unwrap();

        assert_eq!(store.get(&gas, b"foo").unwrap(), None);
        assert_eq!(store.get(&gas, b"food").unwrap(), Some(b"bank".to_vec()));
    }

    #[ignore]
    #[test]
    fn iterator() {
        let storage = RockStore::open("/tmp/foo3");
        let mut store = storage.writer();
        let gas = GasMeter::infinite();
        store.set(&gas, b"foo", b"bar").unwrap();

        // ensure we had previously set "foo" = "bar"
        assert_eq!(store.get(&gas, b"foo").unwrap(), Some(b"bar".to_vec()));
        assert_eq!(store.range(&gas, None, None, Order::Ascending).unwrap().count(), 1);

        // setup - add some data, and delete part of it as well
        store.set(&gas, b"ant", b"hill").unwrap();
        store.set(&gas, b"ze", b"bra").unwrap();

        // noise that should be ignored
        store.set(&gas, b"bye", b"bye").unwrap();
        store.remove(&gas, b"bye").unwrap();

        // unbounded
        {
            let iter = store.range(&gas, None, None, Order::Ascending).unwrap();
            let elements = iter.collect::<Result<Vec<Record>, GasError>>().unwrap();
            assert_eq!(
                elements,
                vec![
                    (b"ant".to_vec(), b"hill".to_vec()),
                    (b"foo".to_vec(), b"bar".to_vec()),
                    (b"ze".to_vec(), b"bra".to_vec()),
                ]
            );
        }

        // unbounded (descending)
        {
            let iter = store.range(&gas, None, None, Order::Descending).unwrap();
            let elements = iter.collect::<Result<Vec<Record>, GasError>>().unwrap();
            assert_eq!(
                elements,
                vec![
                    (b"ze".to_vec(), b"bra".to_vec()),
                    (b"foo".to_vec(), b"bar".to_vec()),
                    (b"ant".to_vec(), b"hill".to_vec()),
                ]
            );
        }

        // bounded
        {
            let iter = store.range(&gas, Some(b"f"), Some(b"n"), Order::Ascending).unwrap();
            let elements = iter.collect::<Result<Vec<Record>, GasError>>().unwrap();
            assert_eq!(elements, vec![(b"foo".to_vec(), b"bar".to_vec())]);
        }

        // bounded (descending)
        {
            let iter = store.range(&gas, Some(b"air"), Some(b"loop"), Order::Descending).unwrap();
            let elements = iter.collect::<Result<Vec<Record>, GasError>>().unwrap();
            assert_eq!(
                elements,
                vec![
                    (b"foo".to_vec(), b"bar".to_vec()),
                    (b"ant".to_vec(), b"hill".to_vec()),
                ]
            );
        }

        // bounded empty [a, a)
        {
            let iter = store.range(&gas, Some(b"foo"), Some(b"foo"), Order::Ascending).unwrap();
            let elements = iter.collect::<Result<Vec<Record>, GasError>>().unwrap();
            assert_eq!(elements, vec![]);
        }

        // bounded empty [a, a) (descending)
        {
            let iter = store.range(&gas, Some(b"foo"), Some(b"foo"), Order::Descending).unwrap();
            let elements = iter.collect::<Result<Vec<Record>, GasError>>().unwrap();
            assert_eq!(elements, vec![]);
        }

        // bounded empty [a, b) with b < a
        {
            let iter = store.range(&gas, Some(b"z"), Some(b"a"), Order::Ascending).unwrap();
            let elements = iter.collect::<Result<Vec<Record>, GasError>>().unwrap();
            assert_eq!(elements, vec![]);
        }

        // bounded empty [a, b) with b < a (descending)
        {
            let iter = store.range(&gas, Some(b"z"), Some(b"a"), Order::Descending).unwrap();
            let elements = iter.collect::<Result<Vec<Record>, GasError>>().unwrap();
            assert_eq!(elements, vec![]);
        }

        // right unbounded
        {
            let iter = store.range(&gas, Some(b"f"), None, Order::Ascending).unwrap();
            let elements = iter.collect::<Result<Vec<Record>, GasError>>().unwrap();
            assert_eq!(
                elements,
                vec![
                    (b"foo".to_vec(), b"bar".to_vec()),
                    (b"ze".to_vec(), b"bra".to_vec()),
                ]
            );
        }

        // right unbounded (descending)
        {
            let iter = store.range(&gas, Some(b"f"), None, Order::Descending).unwrap();
            let elements = iter.collect::<Result<Vec<Record>, GasError>>().unwrap();
            assert_eq!(
                elements,
                vec![
                    (b"ze".to_vec(), b"bra".to_vec()),
                    (b"foo".to_vec(), b"bar".to_vec()),
                ]
            );
        }

        // left unbounded
        {
            let iter = store.range(&gas, None, Some(b"f"), Order::Ascending).unwrap();
            let elements = iter.collect::<Result<Vec<Record>, GasError>>().unwrap();
            assert_eq!(elements, vec![(b"ant".to_vec(), b"hill".to_vec()),]);
        }

        // left unbounded (descending)
        {
            let iter = store.range(&gas, None, Some(b"no"), Order::Descending).unwrap();
            let elements = iter.collect::<Result<Vec<Record>, GasError>>().unwrap();
            assert_eq!(
                elements,
                vec![
                    (b"foo".to_vec(), b"bar".to_vec()),
                    (b"ant".to_vec(), b"hill".to_vec()),
                ]
            );
        }
    }
}