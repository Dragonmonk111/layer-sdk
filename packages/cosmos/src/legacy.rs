use cosmwasm_std::{Binary, StdError};
use serde::Serialize;
use serde_json::value::RawValue;

use pulsar_std::{BankMsg, FeeInfo, Msg, WasmMsg};

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

/// Reference definitions from CosmJS encoder
/// https://github.com/cosmos/cosmjs/blob/main/packages/cosmwasm-stargate/src/modules/wasm/aminomessages.ts
/// https://github.com/cosmos/cosmjs/blob/main/packages/stargate/src/modules/bank/aminomessages.ts
#[derive(Serialize, Debug)]
#[serde(tag = "type", content = "value")]
pub enum AminoMsg {
    #[serde(rename = "cosmos-sdk/MsgSend")]
    MsgSend(AminoMsgSend),
    #[serde(rename = "wasm/MsgExecuteContract")]
    MsgExecute(AminoMsgExecute),
    #[serde(rename = "wasm/MsgInstantiateContract")]
    MsgInstantiate(AminoMsgInstantiate),
    // #[serde(rename = "wasm/MsgInstantiateContract2")]
    // MsgInstantiate2(AminoMsgInstantiate2),
    // #[serde(rename = "wasm/MsgMigrateContract")]
    // MsgMigrate(AminoMsgMigrate),
    // #[serde(rename = "wasm/MsgUpdateAdmin")]
    // MsgUpdateAdmin(AminoMsgUpdateAdmin),
    // #[serde(rename = "wasm/MsgClearAdmin")]
    // MsgClearAdmin(AminoMsgClearAdmin),
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
            Msg::Wasm(WasmMsg::Execute {
                contract_addr,
                msg,
                sender,
                funds,
            }) => AminoMsg::MsgExecute(AminoMsgExecute {
                funds: funds.iter().map(Into::into).collect(),
                sender: sender.to_string(),
                contract: contract_addr.to_string(),
                msg: convert_message(msg),
            }),
            Msg::Wasm(WasmMsg::Instantiate {
                msg,
                sender,
                funds,
                admin,
                code_id,
                label,
            }) => AminoMsg::MsgInstantiate(AminoMsgInstantiate {
                funds: funds.iter().map(Into::into).collect(),
                sender: sender.to_string(),
                msg: convert_message(msg),
                admin: admin.as_ref().map(|a| a.to_string()),
                code_id: code_id.to_string(),
                label: label.to_string(),
            }),
            _ => AminoMsg::Other(()),
        }
    }
}

// TODO: test this
fn convert_message(msg: &Binary) -> Box<RawValue> {
    let val: &RawValue = serde_json::from_slice(msg).unwrap();
    val.to_owned()
}

#[derive(Serialize, Debug)]
pub struct AminoMsgSend {
    pub amount: Vec<Coin>,
    pub from_address: String,
    pub to_address: String,
}

#[derive(Serialize, Debug)]
pub struct AminoMsgExecute {
    pub funds: Vec<Coin>,
    pub contract: String,
    pub msg: Box<RawValue>,
    pub sender: String,
}

#[derive(Serialize, Debug)]
pub struct AminoMsgInstantiate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub admin: Option<String>,
    pub code_id: String,
    pub funds: Vec<Coin>,
    pub label: String,
    pub msg: Box<RawValue>,
    pub sender: String,
}

#[cfg(test)]
mod tests {
    use pulsar_std::AccountId;

    use super::*;

    #[derive(Serialize)]
    pub struct DemoMsg {
        pub age: u32,
        pub height: Option<u32>,
        pub name: String,
    }

    #[test]
    fn check_convert_message() {
        let orig_msg = DemoMsg {
            name: "John Smith".into(),
            age: 32,
            height: Some(187),
        };
        let msg: Binary = serde_json::to_vec(&orig_msg).unwrap().into();
        let raw = convert_message(&msg);
        assert_eq!(raw.get(), r#"{"age":32,"height":187,"name":"John Smith"}"#);
    }

    #[test]
    fn check_convert_execute() {
        let orig_msg = DemoMsg {
            name: "John Smith".into(),
            age: 32,
            height: Some(187),
        };
        let exec_msg = Msg::Wasm(WasmMsg::Execute { 
            sender: AccountId::unchecked("slay3r1funkychicken"), 
            contract_addr: AccountId::unchecked("slay3r1blackholeson"), 
            msg: serde_json::to_vec(&orig_msg).unwrap().into(), 
            funds: vec![],
        });
        let amino_msg = AminoMsg::build(&exec_msg);
        let output = serde_json::to_string(&amino_msg).unwrap();
        assert_eq!(output.as_str(), r#"{"age":32,"height":187,"name":"John Smith"}"#);
    }

}