use cosmwasm_std::{Order, Record};
use pulsar_std::{GasMeter, GasResult};

use super::ReaderWrapper;
use crate::{ReadonlyStorage, Storage, WriteTx};

/// This wraps ReadonlyStorage, providing a scratch-pad that acts like normal Storage
/// but can never be persisted to the underlying state.
///
/// Useful for things like CheckTx and SimulateQuery that may execute code that normally
/// updates state, but do so as part of a read-only transaction.
pub struct ScratchTx<'a> {
    storage: &'a dyn ReadonlyStorage,
    wrap: ReaderWrapper,
}

impl<'a> ScratchTx<'a> {
    pub fn new(storage: &'a dyn ReadonlyStorage) -> Self {
        ScratchTx {
            storage,
            wrap: ReaderWrapper::new(),
        }
    }
}

impl ReadonlyStorage for ScratchTx<'_> {
    fn get(&self, meter: &mut GasMeter, key: &[u8]) -> GasResult<Option<Vec<u8>>> {
        self.wrap.get(self.storage, meter, key)
    }

    fn range<'a>(
        &'a self,
        meter: &'a mut GasMeter,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: Order,
    ) -> GasResult<Box<dyn Iterator<Item = GasResult<Record>> + 'a>> {
        self.wrap.range(self.storage, meter, start, end, order)
    }

    fn abort(self) {}

    fn scratch_tx<'b>(&'b self) -> Box<dyn ReadonlyStorage + 'b> {
        Box::new(crate::ScratchTx::new(self))
    }
}

impl Storage for ScratchTx<'_> {
    fn set(&mut self, meter: &mut GasMeter, key: &[u8], value: &[u8]) -> GasResult<()> {
        self.wrap.set(meter, key, value)
    }

    fn remove(&mut self, meter: &mut GasMeter, key: &[u8]) -> GasResult<()> {
        self.wrap.remove(meter, key)
    }

    // FIXME: better error message - this should never be called, but we expose the API for the trait.
    // Shall we make it no op rather than panic??
    fn commit(self, _meter: &mut GasMeter) -> GasResult<()> {
        unimplemented!()
    }

    fn as_ref(&self) -> &dyn ReadonlyStorage {
        self
    }

    fn sub_tx<'b>(&'b mut self) -> Box<dyn Storage + 'b> {
        Box::new(WriteTx::new(self))
    }
}
