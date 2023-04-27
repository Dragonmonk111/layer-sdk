use crate::addr::Addr;
use cosmrs::Any;
use thiserror::Error;

/// This is the internal message format used in Pulsarium.
/// We convert various wire formats into this before processing.
pub enum Msg {}

impl Msg {
    pub fn from_cosmos(msg: &Any) -> Result<Msg, MsgError> {
        // TODO: implement
        Err(MsgError::UnsupportedAnyType(msg.type_url.clone()))
    }

    pub fn required_signers(&self) -> Vec<Addr> {
        // TODO: implement based on reflection of types
        vec![]
    }
}

#[derive(Error, Debug)]
pub enum MsgError {
    #[error("Unsupported Any type: {0}")]
    UnsupportedAnyType(String),
}
