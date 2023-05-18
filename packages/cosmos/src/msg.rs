use cosmos_sdk_proto::{
    cosmos::bank::v1beta1::MsgSend,
    cosmos::base::v1beta1::Coin as SdkCoin,
    traits::{MessageExt, TypeUrl},
};
use cosmrs::Any;
use cosmwasm_std::{Coin, Uint128};

use pulsar_std::{AccountId, BankMsg, Msg, MsgError};

use crate::error::CosmosError;

fn parse_sdk_coin(coin: &SdkCoin) -> Result<Coin, CosmosError> {
    Ok(Coin {
        denom: coin.denom.clone(),
        amount: Uint128::try_from(coin.amount.as_str())?,
    })
}

fn parse_sdk_coins(coins: &[SdkCoin]) -> Result<Vec<Coin>, CosmosError> {
    coins.iter().map(parse_sdk_coin).collect()
}

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
