use crate::api::TxResponse;
use crate::auth::Auth;
use crate::bank::Bank;
use cosmwasm_std::{StdError, Storage};
use pulsar_std::{GasMeter, Msg, Query};

use crate::error::{PulsarError, PulsarResult};

/// This is an immutable State Machine logic that processes incoming transactions.
/// All mutable state held in Storage, which is passed as an argument to these methods.
pub struct StateMachine {
    pub auth: Auth,

    pub bank: Bank,
}

impl StateMachine {
    pub fn new() -> Self {
        StateMachine {
            auth: Auth::new(),
            bank: Bank::new(),
        }
    }

    pub fn query(&self, storage: &dyn Storage, request: Query) -> Result<Vec<u8>, PulsarError> {
        match request {
            Query::Raw { key } => storage
                .get(&key)
                .ok_or_else(|| StdError::not_found("raw").into()),
            Query::Bank(bank) => self.bank.query(storage, bank),
        }
    }

    pub fn process_msg(
        &self,
        storage: &mut dyn Storage,
        gas: &mut GasMeter,
        msg: Msg,
    ) -> PulsarResult<TxResponse> {
        match msg {
            Msg::Bank(bank) => self.bank.process_msg(storage, gas, bank),
        }
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}
