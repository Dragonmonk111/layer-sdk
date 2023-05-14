use cosmwasm_std::{Order, Record};
use parking_lot::{RwLock, RwLockReadGuard};
use pulsar_std::{GasMeter, GasResult};
use std::collections::BTreeMap;
use std::fmt;
use std::iter;
use std::ops::{Bound, RangeBounds};

use crate::wrap::{Op, ReaderWrapper};
use crate::{FastHasher, PersistentStorage, ReadonlyStorage, Storage, WriteTx};

pub struct MemoryStore(RwLock<BTreeStorage>);

struct BTreeStorage {
    hash: Vec<u8>,
    data: BTreeMap<Vec<u8>, Vec<u8>>,
}

impl PersistentStorage for MemoryStore {
    fn reader<'a>(&'a self) -> Box<dyn ReadonlyStorage + 'a> {
        let reader = self.0.read();
        Box::new(MemoryStorageReader(reader))
    }

    fn writer<'a>(&'a self) -> Box<dyn Storage + 'a> {
        Box::new(MemoryStorageWriter::new(self))
    }

    fn app_hash(&self) -> Vec<u8> {
        self.0.read().hash.clone()
    }
}

pub struct MemoryStorageReader<'a>(RwLockReadGuard<'a, BTreeStorage>);

impl ReadonlyStorage for MemoryStorageReader<'_> {
    fn get(&self, meter: &mut GasMeter, key: &[u8]) -> GasResult<Option<Vec<u8>>> {
        self.0.get(meter, key)
    }

    fn range<'a>(
        &'a self,
        meter: &'a mut GasMeter,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: Order,
    ) -> GasResult<Box<dyn Iterator<Item = GasResult<Record>> + 'a>> {
        self.0.range(meter, start, end, order)
    }

    fn abort(self) {
        // nothing to do
    }

    fn scratch_tx<'b>(&'b self) -> Box<dyn ReadonlyStorage + 'b> {
        Box::new(crate::ScratchTx::new(self))
    }
}

pub struct MemoryStorageWriter<'a> {
    // This is needed for commit later on
    persistent: &'a MemoryStore,
    // This is the reader access
    reader: Box<dyn ReadonlyStorage + 'a>,
    // This is a wrapper used for transactions
    wrapper: ReaderWrapper,
}

impl<'a> MemoryStorageWriter<'a> {
    fn new(persistent: &'a MemoryStore) -> Self {
        let reader = persistent.reader();
        Self {
            persistent,
            reader,
            wrapper: ReaderWrapper::new(),
        }
    }
}

impl ReadonlyStorage for MemoryStorageWriter<'_> {
    fn get(&self, meter: &mut GasMeter, key: &[u8]) -> GasResult<Option<Vec<u8>>> {
        self.wrapper.get(self.reader.as_ref(), meter, key)
    }

    fn range<'a>(
        &'a self,
        meter: &'a mut GasMeter,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: Order,
    ) -> GasResult<Box<dyn Iterator<Item = GasResult<Record>> + 'a>> {
        self.wrapper
            .range(self.reader.as_ref(), meter, start, end, order)
    }

    fn abort(self) {
        // nothing to do
    }

    fn scratch_tx<'b>(&'b self) -> Box<dyn ReadonlyStorage + 'b> {
        Box::new(crate::ScratchTx::new(self))
    }
}

impl Storage for MemoryStorageWriter<'_> {
    fn set(&mut self, meter: &mut GasMeter, key: &[u8], value: &[u8]) -> GasResult<()> {
        self.wrapper.set(meter, key, value)
    }

    fn remove(&mut self, meter: &mut GasMeter, key: &[u8]) -> GasResult<()> {
        self.wrapper.remove(meter, key)
    }

    fn commit(self, meter: &mut GasMeter) -> GasResult<()> {
        // start with old app-hash
        let mut hasher = FastHasher::new(&self.persistent.0.read().hash);

        // write all operations to the root storage
        let ops = self.wrapper.prepare();
        let mut writer = self.persistent.0.write();
        for op in ops {
            match op {
                Op::Set { key, value } => {
                    hasher.set(&key, &value);
                    writer.set(meter, key, value)?;
                }
                Op::Delete { key } => {
                    hasher.remove(&key);
                    writer.remove(meter, &key)?;
                }
            }
        }

        // calculate new app hash
        writer.hash = hasher.hash();
        Ok(())
    }
    fn as_ref(&self) -> &dyn ReadonlyStorage {
        self
    }

    fn sub_tx<'b>(&'b mut self) -> Box<dyn Storage + 'b> {
        Box::new(WriteTx::new(self))
    }
}

impl BTreeStorage {
    pub fn new() -> Self {
        BTreeStorage {
            hash: vec![0; 32],
            data: BTreeMap::new(),
        }
    }
}

impl Default for BTreeStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl BTreeStorage {
    fn get(&self, meter: &mut GasMeter, key: &[u8]) -> GasResult<Option<Vec<u8>>> {
        let value = self.data.get(key).cloned();

        // TODO: abstract better
        let val_len = value.as_ref().map(|x| x.len()).unwrap_or_default();
        let cost = 1000u64 + (key.len() + val_len) as u64;
        meter.charge(cost)?;
        Ok(value)
    }

    /// range allows iteration over a set of keys, either forwards or backwards
    /// uses standard rust range notation, and eg db.range(b"foo"..b"bar") also works reverse
    fn range<'a>(
        &'a self,
        meter: &'a mut GasMeter,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: Order,
    ) -> GasResult<Box<dyn Iterator<Item = GasResult<Record>> + 'a>> {
        let bounds = range_bounds(start, end);

        // BTreeMap.range panics if range is start > end.
        // However, this cases represent just empty range and we treat it as such.
        match (bounds.start_bound(), bounds.end_bound()) {
            (Bound::Included(start), Bound::Excluded(end)) if start > end => {
                return Ok(Box::new(iter::empty()));
            }
            _ => {}
        }

        // TODO: abstract better
        let cost = 1000u64;
        meter.charge(cost)?;

        let iter = self
            .data
            .range(bounds)
            .map(|(key, value)| -> GasResult<BTreeMapRecordRef> {
                // TODO: abstract better
                let cost = 1000u64 + (key.len() + value.len()) as u64;
                meter.charge(cost)?;
                Ok((key, value))
            });
        match order {
            Order::Ascending => Ok(Box::new(iter.map(clone_item))),
            Order::Descending => Ok(Box::new(iter.rev().map(clone_item))),
        }
    }

    fn set(&mut self, meter: &mut GasMeter, key: Vec<u8>, value: Vec<u8>) -> GasResult<()> {
        if value.is_empty() {
            panic!("TL;DR: Value must not be empty in Storage::set but in most cases you can use Storage::remove instead. Long story: Getting empty values from storage is not well supported at the moment. Some of our internal interfaces cannot differentiate between a non-existent key and an empty value. Right now, you cannot rely on the behaviour of empty values. To protect you from trouble later on, we stop here. Sorry for the inconvenience! We highly welcome you to contribute to CosmWasm, making this more solid one way or the other.");
        }
        // TODO: abstract better
        let cost = 2000u64 + 2 * (key.len() + value.len()) as u64;
        meter.charge(cost)?;

        // TODO: add to hasher...

        self.data.insert(key, value);
        Ok(())
    }

    fn remove(&mut self, meter: &mut GasMeter, key: &[u8]) -> GasResult<()> {
        // TODO: abstract better
        let cost = 2000u64;
        meter.charge(cost)?;

        // TODO: add to hasher...

        self.data.remove(key);
        Ok(())
    }
}

/// This debug implementation is made for inspecting storages in unit testing.
/// It is made for human readability only and the output can change at any time.
impl fmt::Debug for BTreeStorage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MemoryStorage ({} entries)", self.data.len())?;
        f.write_str(" {\n")?;
        for (key, value) in &self.data {
            f.write_str("  0x")?;
            for byte in key {
                write!(f, "{:02x}", byte)?;
            }
            f.write_str(": 0x")?;
            for byte in value {
                write!(f, "{:02x}", byte)?;
            }
            f.write_str("\n")?;
        }
        f.write_str("}")?;
        Ok(())
    }
}

fn range_bounds(start: Option<&[u8]>, end: Option<&[u8]>) -> impl RangeBounds<Vec<u8>> {
    (
        start.map_or(Bound::Unbounded, |x| Bound::Included(x.to_vec())),
        end.map_or(Bound::Unbounded, |x| Bound::Excluded(x.to_vec())),
    )
}

/// The BTreeMap specific key-value pair reference type, as returned by BTreeMap<Vec<u8>, Vec<u8>>::range.
/// This is internal as it can change any time if the map implementation is swapped out.
type BTreeMapRecordRef<'a> = (&'a Vec<u8>, &'a Vec<u8>);

fn clone_item(item_ref: GasResult<BTreeMapRecordRef>) -> GasResult<Record> {
    item_ref.map(|(key, value)| (key.clone(), value.clone()))
}

/*
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_and_set() {
        let mut store = BTreeStorage::new();
        assert_eq!(store.get(b"foo"), None);
        store.set(b"foo", b"bar");
        assert_eq!(store.get(b"foo"), Some(b"bar".to_vec()));
        assert_eq!(store.get(b"food"), None);
    }

    #[test]
    #[should_panic(
        expected = "Getting empty values from storage is not well supported at the moment."
    )]
    fn set_panics_for_empty() {
        let mut store = BTreeStorage::new();
        store.set(b"foo", b"");
    }

    #[test]
    fn delete() {
        let mut store = BTreeStorage::new();
        store.set(b"foo", b"bar");
        store.set(b"food", b"bank");
        store.remove(b"foo");

        assert_eq!(store.get(b"foo"), None);
        assert_eq!(store.get(b"food"), Some(b"bank".to_vec()));
    }

    #[test]
    fn iterator() {
        let mut store = BTreeStorage::new();
        store.set(b"foo", b"bar");

        // ensure we had previously set "foo" = "bar"
        assert_eq!(store.get(b"foo"), Some(b"bar".to_vec()));
        assert_eq!(store.range(None, None, Order::Ascending).count(), 1);

        // setup - add some data, and delete part of it as well
        store.set(b"ant", b"hill");
        store.set(b"ze", b"bra");

        // noise that should be ignored
        store.set(b"bye", b"bye");
        store.remove(b"bye");

        // unbounded
        {
            let iter = store.range(None, None, Order::Ascending);
            let elements: Vec<Record> = iter.collect();
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
            let iter = store.range(None, None, Order::Descending);
            let elements: Vec<Record> = iter.collect();
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
            let iter = store.range(Some(b"f"), Some(b"n"), Order::Ascending);
            let elements: Vec<Record> = iter.collect();
            assert_eq!(elements, vec![(b"foo".to_vec(), b"bar".to_vec())]);
        }

        // bounded (descending)
        {
            let iter = store.range(Some(b"air"), Some(b"loop"), Order::Descending);
            let elements: Vec<Record> = iter.collect();
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
            let iter = store.range(Some(b"foo"), Some(b"foo"), Order::Ascending);
            let elements: Vec<Record> = iter.collect();
            assert_eq!(elements, vec![]);
        }

        // bounded empty [a, a) (descending)
        {
            let iter = store.range(Some(b"foo"), Some(b"foo"), Order::Descending);
            let elements: Vec<Record> = iter.collect();
            assert_eq!(elements, vec![]);
        }

        // bounded empty [a, b) with b < a
        {
            let iter = store.range(Some(b"z"), Some(b"a"), Order::Ascending);
            let elements: Vec<Record> = iter.collect();
            assert_eq!(elements, vec![]);
        }

        // bounded empty [a, b) with b < a (descending)
        {
            let iter = store.range(Some(b"z"), Some(b"a"), Order::Descending);
            let elements: Vec<Record> = iter.collect();
            assert_eq!(elements, vec![]);
        }

        // right unbounded
        {
            let iter = store.range(Some(b"f"), None, Order::Ascending);
            let elements: Vec<Record> = iter.collect();
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
            let iter = store.range(Some(b"f"), None, Order::Descending);
            let elements: Vec<Record> = iter.collect();
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
            let iter = store.range(None, Some(b"f"), Order::Ascending);
            let elements: Vec<Record> = iter.collect();
            assert_eq!(elements, vec![(b"ant".to_vec(), b"hill".to_vec()),]);
        }

        // left unbounded (descending)
        {
            let iter = store.range(None, Some(b"no"), Order::Descending);
            let elements: Vec<Record> = iter.collect();
            assert_eq!(
                elements,
                vec![
                    (b"foo".to_vec(), b"bar".to_vec()),
                    (b"ant".to_vec(), b"hill".to_vec()),
                ]
            );
        }
    }

    #[test]
    fn memory_storage_implements_debug() {
        let store = BTreeStorage::new();
        assert_eq!(
            format!("{:?}", store),
            "MemoryStorage (0 entries) {\n\
            }"
        );

        // With one element
        let mut store = BTreeStorage::new();
        store.set(&[0x00, 0xAB, 0xDD], &[0xFF, 0xD5]);
        assert_eq!(
            format!("{:?}", store),
            "MemoryStorage (1 entries) {\n\
            \x20\x200x00abdd: 0xffd5\n\
            }"
        );

        // Sorted by key
        let mut store = BTreeStorage::new();
        store.set(&[0x00, 0xAB, 0xDD], &[0xFF, 0xD5]);
        store.set(&[0x00, 0xAB, 0xEE], &[0xFF, 0xD5]);
        store.set(&[0x00, 0xAB, 0xCC], &[0xFF, 0xD5]);
        assert_eq!(
            format!("{:?}", store),
            "MemoryStorage (3 entries) {\n\
            \x20\x200x00abcc: 0xffd5\n\
            \x20\x200x00abdd: 0xffd5\n\
            \x20\x200x00abee: 0xffd5\n\
            }"
        );

        // Different lengths
        let mut store = BTreeStorage::new();
        store.set(&[0xAA], &[0x11]);
        store.set(&[0xAA, 0xBB], &[0x11, 0x22]);
        store.set(&[0xAA, 0xBB, 0xCC], &[0x11, 0x22, 0x33]);
        store.set(&[0xAA, 0xBB, 0xCC, 0xDD], &[0x11, 0x22, 0x33, 0x44]);
        assert_eq!(
            format!("{:?}", store),
            "MemoryStorage (4 entries) {\n\
            \x20\x200xaa: 0x11\n\
            \x20\x200xaabb: 0x1122\n\
            \x20\x200xaabbcc: 0x112233\n\
            \x20\x200xaabbccdd: 0x11223344\n\
            }"
        );
    }
}
*/
