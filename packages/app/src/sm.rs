use cosmwasm_std::{StdError, Storage};
use pulsar_std::Query;

use crate::error::PulsarError;

/// This is an immutable State Machine logic that processes incoming transactions.
/// All mutable state held in Storage, which is passed as an argument to these methods.
pub struct StateMachine {
    // TODO: auth, bank, etc
}

impl StateMachine {
    pub fn query(&self, storage: &dyn Storage, request: Query) -> Result<Vec<u8>, PulsarError> {
        match request {
            Query::Raw { key } => storage
                .get(&key)
                .ok_or_else(|| StdError::not_found("raw").into()),
            _ => todo!(),
        }
    }
}
