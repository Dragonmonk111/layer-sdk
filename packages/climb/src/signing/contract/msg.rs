use anyhow::Result;
use serde::Serialize;

use crate::{contract_helpers::contract_msg_to_vec, prelude::*};

impl SigningClient {
    pub fn contract_upload_file_msg(
        &self,
        wasm_byte_code: Vec<u8>,
    ) -> Result<proto::wasm::MsgStoreCode> {
        Ok(proto::wasm::MsgStoreCode {
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
        funds: Vec<proto::Coin>,
        msg: &impl Serialize,
    ) -> Result<proto::wasm::MsgInstantiateContract> {
        Ok(proto::wasm::MsgInstantiateContract {
            sender: self.addr.to_string(),
            admin: admin.into().map(|a| a.to_string()).unwrap_or_default(),
            code_id,
            label: label.to_string(),
            msg: contract_msg_to_vec(msg)?,
            funds,
        })
    }

    pub fn contract_execute_msg(
        &self,
        address: &Address,
        funds: Vec<proto::Coin>,
        msg: &impl Serialize,
    ) -> Result<proto::wasm::MsgExecuteContract> {
        Ok(proto::wasm::MsgExecuteContract {
            sender: self.addr.to_string(),
            contract: address.to_string(),
            msg: contract_msg_to_vec(msg)?,
            funds,
        })
    }

    pub fn contract_migrate_msg(
        &self,
        address: &Address,
        code_id: u64,
        msg: &impl Serialize,
    ) -> Result<proto::wasm::MsgMigrateContract> {
        Ok(proto::wasm::MsgMigrateContract {
            sender: self.addr.to_string(),
            contract: address.to_string(),
            code_id,
            msg: contract_msg_to_vec(msg)?,
        })
    }
}
