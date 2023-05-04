mod error;

use cosmwasm_std::Storage;
use pulsar_std::{Msg, Tx};

pub use error::AuthError;

pub struct Auth {
    // TODO
}

impl Auth {
    pub fn new() -> Self {
        Auth {}
    }

    // needs mutable storage for sequence
    pub fn validate_tx(&self, _storage: &mut dyn Storage, _tx: Tx) -> Result<TxData, AuthError> {
        todo!()
    }
}

impl Default for Auth {
    fn default() -> Self {
        Self::new()
    }
}

// info on a validated transaction
pub struct TxData {
    pub msgs: Vec<Msg>,

    pub gas_wanted: u64,
    // TODO: include fee info here? or do we charge directly inside?
}
