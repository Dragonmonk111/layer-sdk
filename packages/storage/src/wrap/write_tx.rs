use cosmwasm_std::{Order, Record};
use pulsar_std::{GasMeter, GasResult};

use super::{Delta, ReaderWrapper};
use crate::{ReadonlyStorage, Storage};

/// This can wrap any Storage with another one as temporary transaction level on top.
/// If aborted, all writes will be discarded.
/// If committed, all writes will be written to the underlying storage.
/// Holds an exclusive lock (&mut) on the Storage until committed or aborted
pub struct WriteTx<'a> {
    storage: &'a mut dyn Storage,
    wrap: ReaderWrapper,
}

impl<'a> WriteTx<'a> {
    pub fn new(storage: &'a mut dyn Storage) -> Self {
        WriteTx {
            storage,
            wrap: ReaderWrapper::new(),
        }
    }
}

impl ReadonlyStorage for WriteTx<'_> {
    fn get(&self, meter: &mut GasMeter, key: &[u8]) -> GasResult<Option<Vec<u8>>> {
        self.wrap.get_mut(self.storage, meter, key)
    }

    fn range<'a>(
        &'a self,
        meter: &'a mut GasMeter,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: Order,
    ) -> GasResult<Box<dyn Iterator<Item = GasResult<Record>> + 'a>> {
        self.wrap.range_mut(self.storage, meter, start, end, order)
    }

    fn abort(self) -> () {}
}

impl Storage for WriteTx<'_> {
    fn set(&mut self, meter: &mut GasMeter, key: &[u8], value: &[u8]) -> GasResult<()> {
        self.wrap.set(meter, key, value)
    }

    fn remove(&mut self, meter: &mut GasMeter, key: &[u8]) -> GasResult<()> {
        self.wrap.remove(meter, key)
    }

    // FIXME: better error message - this should never be called, but we expose the API for the trait.
    // Shall we make it no op rather than panic??
    fn commit(self, meter: &mut GasMeter) -> GasResult<()> {
        // Write directly to underlying storage without intermediate vector
        for (key, delta) in self.wrap.local_state.into_iter() {
            match delta {
                Delta::Set { value } => {
                    self.storage.set(meter, &key, &value)?;
                }
                Delta::Delete {} => {
                    self.storage.remove(meter, &key)?;
                }
            }
        }
        Ok(())
    }
}

/// Ugly helper
struct CastReadonly<'a>(&'a dyn Storage);

impl<'a> ReadonlyStorage for CastReadonly<'a> {
    fn get(&self, meter: &mut GasMeter, key: &[u8]) -> GasResult<Option<Vec<u8>>> {
        self.0.get(meter, key)
    }

    fn range<'b>(
        &'b self,
        meter: &'b mut GasMeter,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: Order,
    ) -> GasResult<Box<dyn Iterator<Item = GasResult<Record>> + 'b>> {
        self.0.range(meter, start, end, order)
    }

    fn abort(self) -> () {
        // intentionally not implemented
        unimplemented!()
    }
}
