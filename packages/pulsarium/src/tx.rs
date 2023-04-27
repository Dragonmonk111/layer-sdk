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

/// Information to execute the contents of the transaction after it has passed auth
pub struct ExecInfo {
    pub msgs: Vec<Msg>,
    pub signers: Vec<Addr>,
}

impl Tx {
    pub fn messages(&self) -> Result<ExecInfo, TxError> {
        match self {
            Tx::Cosmos(tx) => parse_cosmos_tx(tx),
        }
    }

    // TODO: add helpers to get signing info for auth
}

// TODO: make this a method on CosmosTx?
fn parse_cosmos_tx(tx: &CosmosTx) -> Result<ExecInfo, TxError> {
    let msgs = tx
        .body
        .messages
        .iter()
        .map(|msg| Msg::from_cosmos(msg))
        .collect::<Result<Vec<_>, MsgError>>()?;
    // need to get required signers from those messages... arg!
    let signers = msgs.iter().flat_map(|m| m.required_signers()).collect();
    Ok(ExecInfo { msgs, signers })
}

#[derive(Error, Debug)]
pub enum TxError {
    #[error("{0}")]
    Msg(#[from] MsgError),
}
