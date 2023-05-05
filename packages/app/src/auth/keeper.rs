use crate::error::PulsarResult;
use cosmwasm_std::{BlockInfo, Storage};
use pulsar_std::{Addr, Msg, Tx};

use crate::sm::StateMachine;

pub struct Auth {
    // TODO
}

impl Auth {
    pub fn new() -> Self {
        Auth {}
    }

    /// This checks (and bumps) sequences of the local storage and deducts the fees from the account.
    /// If successful, it returns the TxData with all info that needs to be executed.
    pub fn validate_tx(
        &self,
        _storage: &mut dyn Storage,
        _block: &BlockInfo,
        _sm: &StateMachine,
        _tx: Tx,
    ) -> PulsarResult<TxData> {
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
    /// We only support one sender per transaction
    pub signer: Addr,

    /// All messages in the payload, decoded to Pulsarium format
    pub msgs: Vec<Msg>,

    /// Amount of gas this transaction may use
    pub gas_wanted: u64,
}
