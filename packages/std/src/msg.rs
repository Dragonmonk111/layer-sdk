use cosmwasm_std::{Coin, StdError};
use itertools::Itertools;
use std::fmt::{Display, Formatter};
use thiserror::Error;

use crate::account_id::{AccountId, AccountIdError};

/// This is the internal message format used in Pulsarium.
/// We convert various wire formats into this before processing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    Bank(BankMsg),
}

impl From<BankMsg> for Msg {
    fn from(value: BankMsg) -> Self {
        Msg::Bank(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BankMsg {
    Send {
        sender: AccountId,
        recipient: AccountId,
        amount: Vec<Coin>,
    },
    Burn {
        sender: AccountId,
        amount: Vec<Coin>,
    },
}

impl Display for BankMsg {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BankMsg::Send { .. } => f.write_str("BankMsg::Send"),
            BankMsg::Burn { .. } => f.write_str("BankMsg::Burn"),
        }
    }
}

#[derive(Error, Debug, PartialEq)]
pub enum MsgError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unsupported Any type: {0}")]
    UnsupportedAnyType(String),

    #[error("Tx doesn't have any messages")]
    NoMessages,

    #[error("Tx requires signatures from multiple addresses - not supported")]
    MultipleSigners,

    #[error("{0}")]
    Addr(#[from] AccountIdError),

    /// FIXME: either ensure all callers of this function produce determinstic strings,
    /// Or remove all info
    #[error("Parse: {0}")]
    ParseError(String),
}

impl Msg {
    /// List which addresses must sign the message for it to be valid
    pub fn required_signer(&self) -> AccountId {
        match &self {
            Msg::Bank(BankMsg::Send { sender, .. }) => sender.clone(),
            Msg::Bank(BankMsg::Burn { sender, .. }) => sender.clone(),
        }
    }
}

/// Returns the signer needed by all Messages.
/// If there are no messages, or different signers required by messages, returns an error
pub fn required_signer(msgs: &[Msg]) -> Result<AccountId, MsgError> {
    let mut signers: Vec<_> = msgs.iter().map(Msg::required_signer).dedup().collect();
    if signers.len() > 1 {
        return Err(MsgError::MultipleSigners);
    }
    signers.pop().ok_or(MsgError::NoMessages)
}
