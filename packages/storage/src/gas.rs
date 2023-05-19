use cosmwasm_std::{Order, Record};

use pulsar_std::{GasError, GasMeter, GasResult};

use crate::Storage;

/// Similar to cosmwasm_std::Storage, but with Results in return values
pub trait GasStorage {
    fn get(&mut self, key: &[u8]) -> GasResult<Option<Vec<u8>>>;

    fn range<'a>(
        &'a mut self,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: Order,
    ) -> GasResult<Box<dyn Iterator<Item = GasResult<Record>> + 'a>>;

    fn set(&mut self, key: &[u8], value: &[u8]) -> GasResult<()>;

    fn remove(&mut self, key: &[u8]) -> GasResult<()>;

    fn charge_gas(&mut self, gas: u64) -> Result<(), GasError>;
}

// FIXME: if used, add readonly variant
pub struct PulsarStorage<'a> {
    storage: &'a mut dyn Storage,
    meter: &'a mut GasMeter,
}

impl<'a> PulsarStorage<'a> {
    pub fn new(storage: &'a mut dyn Storage, meter: &'a mut GasMeter) -> Self {
        PulsarStorage { storage, meter }
    }
}

impl GasStorage for PulsarStorage<'_> {
    fn get(&mut self, key: &[u8]) -> GasResult<Option<Vec<u8>>> {
        self.storage.get(self.meter, key)
    }

    fn range<'a>(
        &'a mut self,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: Order,
    ) -> GasResult<Box<dyn Iterator<Item = GasResult<Record>> + 'a>> {
        self.storage.range(self.meter, start, end, order)
    }

    fn set(&mut self, key: &[u8], value: &[u8]) -> GasResult<()> {
        self.storage.set(self.meter, key, value)
    }

    fn remove(&mut self, key: &[u8]) -> GasResult<()> {
        self.storage.remove(self.meter, key)
    }

    fn charge_gas(&mut self, gas: u64) -> Result<(), GasError> {
        self.meter.charge(gas)
    }
}
