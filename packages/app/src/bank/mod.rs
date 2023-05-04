mod error;

use cosmwasm_std::Storage;
use pulsar_std::{BankMsg, BankQuery, GasMeter};

use crate::api::TxResponse;
use crate::error::PulsarResult;
pub use error::BankError;

pub struct Bank {
    // TODO
}

impl Bank {
    pub fn new() -> Self {
        Bank {}
    }

    pub fn process_msg(
        &self,
        _storage: &mut dyn Storage,
        _meter: &mut GasMeter,
        _msg: BankMsg,
    ) -> PulsarResult<TxResponse> {
        todo!()
    }

    pub fn query(&self, _storage: &dyn Storage, _request: BankQuery) -> PulsarResult<Vec<u8>> {
        todo!()
    }
}

impl Default for Bank {
    fn default() -> Self {
        Self::new()
    }
}
