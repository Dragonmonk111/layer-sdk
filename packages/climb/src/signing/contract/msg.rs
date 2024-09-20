use anyhow::Result;
use serde::Serialize;

use crate::prelude::*;

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
        params: InstantiateParams<'_, impl Serialize>,
    ) -> Result<cosmrs::proto::cosmwasm::wasm::v1::MsgInstantiateContract> {
        let InstantiateParams {
            admin,
            code_id,
            label,
            funds,
            msg,
        } = params;

        Ok(cosmrs::proto::cosmwasm::wasm::v1::MsgInstantiateContract {
            sender: self.addr.to_string(),
            admin: admin.map(|a| a.to_string()).unwrap_or_default(),
            code_id,
            label: label.to_string(),
            msg: msg.try_into_vec()?,
            funds: funds.unwrap_or_default(),
        })
    }

    pub fn contract_migrate_msg(
        &self,
        params: MigrateParams<'_, impl Serialize>,
    ) -> Result<cosmrs::proto::cosmwasm::wasm::v1::MsgMigrateContract> {
        let MigrateParams {
            address,
            code_id,
            msg,
        } = params;

        Ok(cosmrs::proto::cosmwasm::wasm::v1::MsgMigrateContract {
            sender: self.addr.to_string(),
            contract: address.to_string(),
            code_id,
            msg: msg.try_into_vec()?,
        })
    }

    pub fn contract_execute_msg(
        &self,
        params: ExecuteParams<'_, impl Serialize>,
    ) -> Result<cosmrs::proto::cosmwasm::wasm::v1::MsgExecuteContract> {
        let ExecuteParams {
            address,
            funds,
            msg,
        } = params;

        Ok(cosmrs::proto::cosmwasm::wasm::v1::MsgExecuteContract {
            sender: self.addr.to_string(),
            contract: address.to_string(),
            msg: msg.try_into_vec()?,
            funds: funds.unwrap_or_default(),
        })
    }
}

pub struct InstantiateParams<'a, T: Serialize> {
    pub admin: Option<Address>,
    pub code_id: u64,
    pub label: String,
    pub funds: Option<Vec<cosmrs::proto::cosmos::base::v1beta1::Coin>>,
    pub msg: ContractMessage<'a, T>,
}

impl<'a, T: Serialize> InstantiateParams<'a, T> {
    pub fn new(code_id: u64, label: String, msg: ContractMessage<'a, T>) -> Self {
        Self {
            admin: None,
            code_id,
            label,
            funds: None,
            msg,
        }
    }

    pub fn set_funds(mut self, funds: Vec<cosmrs::proto::cosmos::base::v1beta1::Coin>) -> Self {
        self.funds = Some(funds);
        self
    }

    pub fn set_admin(mut self, admin: Address) -> Self {
        self.admin = Some(admin);
        self
    }
}

pub struct ExecuteParams<'a, T: Serialize> {
    pub address: Address,
    pub funds: Option<Vec<cosmrs::proto::cosmos::base::v1beta1::Coin>>,
    pub msg: ContractMessage<'a, T>,
}

impl<'a, T: Serialize> ExecuteParams<'a, T> {
    pub fn new(address: Address, msg: ContractMessage<'a, T>) -> Self {
        Self {
            address,
            funds: None,
            msg,
        }
    }

    pub fn set_funds(mut self, funds: Vec<cosmrs::proto::cosmos::base::v1beta1::Coin>) -> Self {
        self.funds = Some(funds);
        self
    }
}

pub struct MigrateParams<'a, T: Serialize> {
    pub address: Address,
    pub code_id: u64,
    pub msg: ContractMessage<'a, T>,
}

impl<'a, T: Serialize> MigrateParams<'a, T> {
    pub fn new(address: Address, code_id: u64, msg: ContractMessage<'a, T>) -> Self {
        Self {
            address,
            code_id,
            msg,
        }
    }
}
