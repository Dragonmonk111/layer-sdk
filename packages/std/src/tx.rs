pub use cosmrs::Tx as CosmosTx;
use thiserror::Error;

use crate::addr::Addr;
use crate::msg::{Msg, MsgError};

/// A list of various tx formats we accept.
/// We start with Cosmos-SDK format for compatibility, but want to later allow native signing format.
/// We can pass this around to auth to allow handling multiple types
pub enum Tx {
    /// Cosmos Format. Note that we support a subset of the functionality:
    /// Only one signer, no authz or fee grants. But that means 90%+ of tx work, and are "Keplr compatible"
    Cosmos(CosmosTx),
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum TxError {
    #[error("{0}")]
    Msg(#[from] MsgError),
}

/// Information to execute the contents of the transaction after it has passed auth
pub struct ExecInfo {
    pub msgs: Vec<Msg>,
    pub signer: Addr,
}

impl Tx {
    pub fn parse_tx(&self) -> Result<ExecInfo, TxError> {
        match self {
            Tx::Cosmos(tx) => cosmos::parse_tx(tx),
        }
    }
}

mod cosmos {
    use super::*;
    use crate::required_signer;

    pub fn parse_tx(tx: &CosmosTx) -> Result<ExecInfo, TxError> {
        let msgs: Result<Vec<_>, _> = tx.body.messages.iter().map(Msg::from_cosmos).collect();
        let msgs = msgs?;
        let signer = required_signer(&msgs)?;
        Ok(ExecInfo { msgs, signer })
    }
}
