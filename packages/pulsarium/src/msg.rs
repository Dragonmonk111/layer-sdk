use crate::addr::{Addr, AddrError};
use cosmos_sdk_proto::{
    cosmos::bank::v1beta1::MsgSend,
    cosmos::base::v1beta1::Coin as SdkCoin,
    prost::DecodeError,
    traits::{MessageExt, TypeUrl},
};

use cosmrs::Any;
use cosmwasm_std::Coin;
use thiserror::Error;

/// This is the internal message format used in Pulsarium.
/// We convert various wire formats into this before processing.
pub enum Msg {
    Bank(BankMsg),
}

pub enum BankMsg {
    Send {
        sender: Addr,
        recipient: Addr,
        amount: Vec<Coin>,
    },
}

impl From<BankMsg> for Msg {
    fn from(value: BankMsg) -> Self {
        Msg::Bank(value)
    }
}

fn parse_sdk_coins(_coins: &[SdkCoin]) -> Result<Vec<Coin>, MsgError> {
    todo!();
}

impl Msg {
    pub fn from_cosmos(msg: &Any) -> Result<Msg, MsgError> {
        match msg.type_url.as_str() {
            MsgSend::TYPE_URL => {
                let parsed = MsgSend::from_any(msg)?;
                Ok(BankMsg::Send {
                    sender: Addr::parse_string(&parsed.from_address)?,
                    recipient: Addr::parse_string(&parsed.to_address)?,
                    amount: parse_sdk_coins(&parsed.amount)?,
                }
                .into())
            }
            _ => Err(MsgError::UnsupportedAnyType(msg.type_url.clone())),
        }
    }

    pub fn required_signers(&self) -> Vec<Addr> {
        match &self {
            Msg::Bank(BankMsg::Send { sender, .. }) => vec![sender.clone()],
        }
    }
}

#[derive(Error, Debug)]
pub enum MsgError {
    #[error("Unsupported Any type: {0}")]
    UnsupportedAnyType(String),

    // TODO: remove this and replace with detereministic errors
    #[error("{0}")]
    ProtoDecode(#[from] DecodeError),

    #[error("{0}")]
    Addr(#[from] AddrError),
}
