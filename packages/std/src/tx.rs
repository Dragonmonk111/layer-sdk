pub use cosmrs::Tx as CosmosTx;
use thiserror::Error;

use crate::addr::Addr;
use crate::msg::{Msg, MsgError};

/// A list of various tx formats we accept.
/// We start with Cosmos-SDK format for compatibility, but want to later allow native signing format.
/// We can pass this around to auth to allow handling multiple types
pub enum Tx {
    Cosmos(CosmosTx),
}

#[derive(Error, Debug)]
pub enum TxError {
    #[error("{0}")]
    Msg(#[from] MsgError),
}

/// Information to execute the contents of the transaction after it has passed auth
pub struct ExecInfo {
    pub msgs: Vec<Msg>,
    pub signers: Vec<Addr>,
}

impl Tx {
    pub fn messages(&self) -> Result<Vec<Msg>, TxError> {
        match self {
            Tx::Cosmos(tx) => cosmos::parse_tx(tx),
        }
    }
}

mod cosmos {
    use super::*;

    // TODO: make this a method on CosmosTx?
    pub fn parse_tx(tx: &CosmosTx) -> Result<Vec<Msg>, TxError> {
        let msgs: Result<Vec<_>, _> = tx.body.messages.iter().map(Msg::from_cosmos).collect();
        Ok(msgs?)
    }
}
