use cosmos_sdk_proto::{
    cosmos::bank::v1beta1::MsgSend,
    traits::{MessageExt, TypeUrl},
};
use cosmrs::Any;

use pulsar_std::{AccountId, BankMsg, Msg, MsgError};

use crate::error::CosmosError;
use crate::utils::parse_sdk_coins;

pub fn parse_cosmos_msg(msg: &Any) -> Result<Msg, MsgError> {
    match msg.type_url.as_str() {
        MsgSend::TYPE_URL => {
            let parsed = MsgSend::from_any(msg).map_err(CosmosError::from)?;
            Ok(BankMsg::Send {
                sender: AccountId::parse_string(&parsed.from_address)?,
                recipient: AccountId::parse_string(&parsed.to_address)?,
                amount: parse_sdk_coins(&parsed.amount)?,
            }
            .into())
        }
        _ => Err(MsgError::UnsupportedAnyType(msg.type_url.clone())),
    }
}
