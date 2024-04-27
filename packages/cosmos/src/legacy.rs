use cosmwasm_std::StdError;
use serde::Serialize;

use pulsar_std::{BankMsg, FeeInfo, Msg};

// We must sort the keys alphabetically to get the "amino serialization"
#[derive(Serialize, Debug)]
pub struct StdSignDoc {
    pub account_number: String,
    pub chain_id: String,
    pub fee: StdFee,
    pub memo: String,
    pub msgs: Vec<AminoMsg>,
    pub sequence: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_height: Option<String>,
}

impl StdSignDoc {
    pub fn build(
        doc: cosmrs::tx::SignDoc,
        msgs: &[Msg],
        fee: &FeeInfo,
        sequence: u64,
        memo: &str,
    ) -> Self {
        let msgs = msgs.iter().map(AminoMsg::build).collect();
        StdSignDoc {
            account_number: doc.account_number.to_string(),
            chain_id: doc.chain_id,
            fee: fee.into(),
            memo: memo.into(), // Do we skip serialization if empty string?
            msgs,
            sequence: sequence.to_string(),
            timeout_height: None,
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, StdError> {
        // Serialize to "sorted" JSON
        let raw =
            serde_json::to_string(self).map_err(|e| StdError::serialize_err("StdSignDoc", e))?;
        // Escape special characters as per amino implementation
        let escaped = raw
            .replace("&", "\\u0026")
            .replace("<", "\\u003c")
            .replace(">", "\\u003e");
        // Turn into bytes
        return Ok(escaped.into_bytes());
    }
}

#[derive(Serialize, Debug)]
pub struct StdFee {
    pub amount: Vec<Coin>,
    pub gas: String,
    // Unsupported for now
    // /** The granter address that is used for paying with feegrants */
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub granter: Option<String>,
    // /** The fee payer address. The payer must have signed the transaction. */
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub payer: Option<String>,
}

impl From<&pulsar_std::FeeInfo> for StdFee {
    fn from(value: &pulsar_std::FeeInfo) -> Self {
        Self {
            amount: value.fee.iter().map(Into::into).collect(),
            gas: value.gas_limit.to_string(),
        }
    }
}

#[derive(Serialize, Debug)]
pub struct Coin {
    pub amount: String,
    pub denom: String,
}

impl From<&cosmrs::Coin> for Coin {
    fn from(value: &cosmrs::Coin) -> Self {
        Coin {
            amount: value.amount.to_string(),
            denom: value.denom.to_string(),
        }
    }
}

impl From<&cosmwasm_std::Coin> for Coin {
    fn from(value: &cosmwasm_std::Coin) -> Self {
        Coin {
            amount: value.amount.to_string(),
            denom: value.denom.to_string(),
        }
    }
}

#[derive(Serialize, Debug)]
#[serde(tag = "type", content = "value")]
pub enum AminoMsg {
    #[serde(rename = "cosmos-sdk/MsgSend")]
    MsgSend(AminoMsgSend),
    #[serde(rename = "bogus/shit")]
    Other(()),
}

impl AminoMsg {
    pub fn build(msg: &Msg) -> Self {
        match msg {
            Msg::Bank(BankMsg::Send {
                sender,
                recipient,
                amount,
            }) => AminoMsg::MsgSend(AminoMsgSend {
                amount: amount.iter().map(Into::into).collect(),
                from_address: sender.to_string(),
                to_address: recipient.to_string(),
            }),
            _ => AminoMsg::Other(()),
        }
    }
}

#[derive(Serialize, Debug)]
pub struct AminoMsgSend {
    pub amount: Vec<Coin>,
    pub from_address: String,
    pub to_address: String,
}
