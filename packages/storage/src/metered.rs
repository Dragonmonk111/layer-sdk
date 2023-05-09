use cosmwasm_std::{Order, Record};
use pulsar_std::{GasMeter, GasResult};

/// This is like cosmwasm_std::Storage, but takes GasMeter as extra arg everywhere
pub trait MeteredStorage {
    fn get(&self, meter: &mut GasMeter, key: &[u8]) -> GasResult<Option<Vec<u8>>>;

    fn range<'a>(
        &'a self,
        meter: &'a mut GasMeter,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: Order,
    ) -> GasResult<Box<dyn Iterator<Item = GasResult<Record>> + 'a>>;

    fn set(&mut self, meter: &mut GasMeter, key: &[u8], value: &[u8]) -> GasResult<()>;

    fn remove(&mut self, meter: &mut GasMeter, key: &[u8]) -> GasResult<()>;
}
