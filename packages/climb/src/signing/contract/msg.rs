use anyhow::Result;
use serde::Serialize;

use crate::{contract_helpers::contract_msg_to_vec, prelude::*};

impl SigningClient {
    pub fn contract_upload_file_msg(
        &self,
        wasm_byte_code: Vec<u8>,
    ) -> Result<cosmrs::proto::cosmwasm::wasm::v1::MsgStoreCode> {
        Ok(cosmrs::proto::cosmwasm::wasm::v1::MsgStoreCode {
            sender: self.addr.to_string(),
            wasm_byte_code,
            instantiate_permission: None,
        })
    }

    pub fn contract_instantiate_msg(
        &self,
        admin: impl Into<Option<Address>>,
        code_id: u64,
        label: impl ToString,
        funds: Option<Vec<cosmrs::proto::cosmos::base::v1beta1::Coin>>,
        msg: &impl Serialize,
    ) -> Result<cosmrs::proto::cosmwasm::wasm::v1::MsgInstantiateContract> {
        Ok(cosmrs::proto::cosmwasm::wasm::v1::MsgInstantiateContract {
            sender: self.addr.to_string(),
            admin: admin.into().map(|a| a.to_string()).unwrap_or_default(),
            code_id,
            label: label.to_string(),
            msg: contract_msg_to_vec(msg)?,
            funds: funds.unwrap_or_default(),
        })
    }

    pub fn contract_execute_msg(
        &self,
        address: &Address,
        funds: Option<Vec<cosmrs::proto::cosmos::base::v1beta1::Coin>>,
        msg: &impl Serialize,
    ) -> Result<cosmrs::proto::cosmwasm::wasm::v1::MsgExecuteContract> {
        Ok(cosmrs::proto::cosmwasm::wasm::v1::MsgExecuteContract {
            sender: self.addr.to_string(),
            contract: address.to_string(),
            msg: contract_msg_to_vec(msg)?,
            funds: funds.unwrap_or_default(),
        })
    }

    pub fn contract_migrate_msg(
        &self,
        address: &Address,
        code_id: u64,
        msg: &impl Serialize,
    ) -> Result<cosmrs::proto::cosmwasm::wasm::v1::MsgMigrateContract> {
        Ok(cosmrs::proto::cosmwasm::wasm::v1::MsgMigrateContract {
            sender: self.addr.to_string(),
            contract: address.to_string(),
            code_id,
            msg: contract_msg_to_vec(msg)?,
        })
    }
}
