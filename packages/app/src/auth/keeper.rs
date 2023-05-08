use crate::error::PulsarResult;
use cosmwasm_std::{BlockInfo, Storage};
use pulsar_std::{AccountId, Msg, Tx};

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
        // ensure this is a cosmos tx

        // load the signer account if any

        // validate the signature with that account (Cosmos-specific), and get gas and fee info

        // filter logic on gas pricing....

        // try to charge fee info

        // bump sequence number

        // Return data
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
    pub signer: AccountId,

    /// All messages in the payload, decoded to Pulsarium format
    pub msgs: Vec<Msg>,

    /// Amount of gas this transaction may use
    pub gas_wanted: u64,
}
