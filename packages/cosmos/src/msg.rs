use cosmos_sdk_proto::{
    cosmos::bank::v1beta1::MsgSend,
    cosmwasm::wasm::v1::{
        MsgClearAdmin, MsgExecuteContract, MsgInstantiateContract, MsgInstantiateContract2,
        MsgMigrateContract, MsgStoreCode, MsgUpdateAdmin,
    },
    prost::Message,
    traits::{MessageExt, TypeUrl},
};
use cosmrs::Any;
use tracing::trace_span;

use layer_std::{AccountId, BankMsg, IbcMsg, Msg, MsgError, WasmMsg};

use crate::error::CosmosError;
use crate::unzip::unzip_if_needed;
use crate::utils::parse_sdk_coins;

const MAX_WASM_SIZE: usize = 1024 * 1024 * 8; // 8 MB (raised for jolt-cw-verifier full-verification + future Akita/lattice path)

pub fn parse_cosmos_msg(msg: &Any) -> Result<Msg, MsgError> {
    let _span = trace_span!("parse_cosmos_msg").entered();
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
        MsgExecuteContract::TYPE_URL => {
            let parsed = MsgExecuteContract::from_any(msg).map_err(CosmosError::from)?;
            Ok(WasmMsg::Execute {
                sender: AccountId::parse_string(&parsed.sender)?,
                contract_addr: AccountId::parse_string(&parsed.contract)?,
                msg: parsed.msg.into(),
                funds: parse_sdk_coins(&parsed.funds)?,
            }
            .into())
        }
        MsgInstantiateContract::TYPE_URL => {
            let parsed = MsgInstantiateContract::from_any(msg).map_err(CosmosError::from)?;
            let admin = if parsed.admin.is_empty() {
                None
            } else {
                Some(AccountId::parse_string(&parsed.admin)?)
            };
            Ok(WasmMsg::Instantiate {
                sender: AccountId::parse_string(&parsed.sender)?,
                admin,
                code_id: parsed.code_id,
                msg: parsed.msg.into(),
                funds: parse_sdk_coins(&parsed.funds)?,
                label: parsed.label,
            }
            .into())
        }
        // FIXME: add this to cosmrs
        // MsgInstantiateContract2::TYPE_URL => {
        "/cosmwasm.wasm.v1.MsgInstantiateContract2" => {
            let parsed = MsgInstantiateContract2::decode(&*msg.value).map_err(CosmosError::from)?;
            let admin = if parsed.admin.is_empty() {
                None
            } else {
                Some(AccountId::parse_string(&parsed.admin)?)
            };
            Ok(WasmMsg::Instantiate2 {
                sender: AccountId::parse_string(&parsed.sender)?,
                admin,
                code_id: parsed.code_id,
                msg: parsed.msg.into(),
                funds: parse_sdk_coins(&parsed.funds)?,
                label: parsed.label,
                salt: parsed.salt.into(),
            }
            .into())
        }
        MsgStoreCode::TYPE_URL => {
            let parsed = MsgStoreCode::from_any(msg).map_err(CosmosError::from)?;
            let code = unzip_if_needed(parsed.wasm_byte_code, MAX_WASM_SIZE)?.into();
            let sender = AccountId::parse_string(&parsed.sender)?;
            Ok(WasmMsg::StoreCode { sender, code }.into())
        }
        MsgMigrateContract::TYPE_URL => {
            let parsed = MsgMigrateContract::from_any(msg).map_err(CosmosError::from)?;
            Ok(WasmMsg::Migrate {
                sender: AccountId::parse_string(&parsed.sender)?,
                contract_addr: AccountId::parse_string(&parsed.contract)?,
                msg: parsed.msg.into(),
                new_code_id: parsed.code_id,
            }
            .into())
        }
        MsgUpdateAdmin::TYPE_URL => {
            let parsed = MsgUpdateAdmin::from_any(msg).map_err(CosmosError::from)?;
            Ok(WasmMsg::UpdateAdmin {
                sender: AccountId::parse_string(&parsed.sender)?,
                contract_addr: AccountId::parse_string(&parsed.contract)?,
                admin: AccountId::parse_string(&parsed.new_admin)?,
            }
            .into())
        }
        MsgClearAdmin::TYPE_URL => {
            let parsed = MsgClearAdmin::from_any(msg).map_err(CosmosError::from)?;
            Ok(WasmMsg::ClearAdmin {
                sender: AccountId::parse_string(&parsed.sender)?,
                contract_addr: AccountId::parse_string(&parsed.contract)?,
            }
            .into())
        }

        // JunoClaw sovereign IBC message: the Any value is a JSON-encoded IbcMsg.
        // The relayer constructs these; the chain decodes + routes to the ibc keeper.
        "/junoclaw.ibc.v1.Msg" => {
            let ibc: IbcMsg = serde_json::from_slice(&msg.value)
                .map_err(|e| MsgError::ParseError(format!("ibc msg decode: {e}")))?;
            Ok(ibc.into())
        }

        _ => Err(MsgError::UnsupportedAnyType(msg.type_url.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reproduce the devnet store_vk path: encode a real proto MsgExecuteContract,
    // decode via parse_cosmos_msg, and confirm the contract msg bytes survive intact.
    #[test]
    fn execute_msg_bytes_survive_proto_decode() {
        let msg_json = br#"{"store_vk":{"vk_base64":"AAAA"}}"#;
        let proto = MsgExecuteContract {
            sender: "juno1dz875zg8p78anpjv3f0qt4gu5a3awpjfhtw992".to_string(),
            contract: "juno17c5ucyukaf9heseh7gnyjgmwd3jtz6gegm039ezkjhnx6zx526hqq0738c"
                .to_string(),
            msg: msg_json.to_vec(),
            funds: vec![],
        };
        let any = proto.to_any().expect("to_any");
        let parsed = parse_cosmos_msg(&any).expect("parse_cosmos_msg");
        match parsed {
            Msg::Wasm(WasmMsg::Execute { msg, .. }) => {
                assert_eq!(
                    msg.as_slice(),
                    &msg_json[..],
                    "contract msg bytes were corrupted in proto decode: {:?}",
                    String::from_utf8_lossy(msg.as_slice())
                );
            }
            other => panic!("expected WasmMsg::Execute, got {:?}", other),
        }
    }
}
