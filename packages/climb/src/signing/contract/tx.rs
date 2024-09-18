use crate::{
    msg_into_cosmrs_any, signing::SigningClient, Address, TxBuilder,
    EVENT_ATTR_INSTANTIATE_CONTRACT_ADDRESS_V1, EVENT_ATTR_INSTANTIATE_CONTRACT_ADDRESS_V2,
    EVENT_ATTR_STORE_CODE_ID, EVENT_TYPE_CONTRACT_INSTANTIATE, EVENT_TYPE_CONTRACT_STORE_CODE,
};

use anyhow::Result;
use serde::Serialize;

use super::msg::{ExecuteParams, InstantiateParams, MigrateParams};
use crate::events::CosmosTxEvents;

impl SigningClient {
    // returns the code id
    pub async fn contract_upload_file(
        &self,
        wasm_byte_code: Vec<u8>,
        tx_builder: Option<TxBuilder<'_>>,
    ) -> Result<(u64, cosmrs::proto::cosmos::base::abci::v1beta1::TxResponse)> {
        let resp = tx_builder
            .unwrap_or_else(|| self.tx_builder())
            .broadcast([msg_into_cosmrs_any(
                &self.contract_upload_file_msg(wasm_byte_code)?,
            )?])
            .await?;

        let code_id: u64 = CosmosTxEvents::from(&resp)
            .attr_first(EVENT_TYPE_CONTRACT_STORE_CODE, EVENT_ATTR_STORE_CODE_ID)?
            .value()
            .parse()?;

        Ok((code_id, resp))
    }

    pub async fn contract_instantiate(
        &self,
        params: InstantiateParams<'_, impl Serialize>,
        tx_builder: Option<TxBuilder<'_>>,
    ) -> Result<(
        Address,
        cosmrs::proto::cosmos::base::abci::v1beta1::TxResponse,
    )> {
        let resp = tx_builder
            .unwrap_or_else(|| self.tx_builder())
            .broadcast([msg_into_cosmrs_any(
                &self.contract_instantiate_msg(params)?,
            )?])
            .await?;

        let events = CosmosTxEvents::from(&resp);

        let contract_address = events
            .attr_first(
                EVENT_TYPE_CONTRACT_INSTANTIATE,
                EVENT_ATTR_INSTANTIATE_CONTRACT_ADDRESS_V1,
            )
            .or_else(|_| {
                events.attr_first(
                    EVENT_TYPE_CONTRACT_INSTANTIATE,
                    EVENT_ATTR_INSTANTIATE_CONTRACT_ADDRESS_V2,
                )
            })?
            .value()
            .to_string();

        let contract_address = self.querier.chain_config.parse_address(contract_address)?;

        Ok((contract_address, resp))
    }

    pub async fn contract_migrate(
        &self,
        params: MigrateParams<'_, impl Serialize>,
        tx_builder: Option<TxBuilder<'_>>,
    ) -> Result<cosmrs::proto::cosmos::base::abci::v1beta1::TxResponse> {
        tx_builder
            .unwrap_or_else(|| self.tx_builder())
            .broadcast([msg_into_cosmrs_any(&self.contract_migrate_msg(params)?)?])
            .await
    }
    pub async fn contract_migrate_multi(
        &self,
        multi_params: Vec<MigrateParams<'_, impl Serialize>>,
        tx_builder: Option<TxBuilder<'_>>,
    ) -> Result<cosmrs::proto::cosmos::base::abci::v1beta1::TxResponse> {
        let msgs = multi_params
            .into_iter()
            .map(|params| {
                self.contract_migrate_msg(params)
                    .and_then(|msg| msg_into_cosmrs_any(&msg))
            })
            .collect::<Result<Vec<_>>>()?;

        tx_builder
            .unwrap_or_else(|| self.tx_builder())
            .broadcast(msgs)
            .await
    }

    pub async fn contract_execute(
        &self,
        params: ExecuteParams<'_, impl Serialize>,
        tx_builder: Option<TxBuilder<'_>>,
    ) -> Result<cosmrs::proto::cosmos::base::abci::v1beta1::TxResponse> {
        tx_builder
            .unwrap_or_else(|| self.tx_builder())
            .broadcast([msg_into_cosmrs_any(&self.contract_execute_msg(params)?)?])
            .await
    }

    pub async fn contract_execute_multi(
        &self,
        multi_params: Vec<ExecuteParams<'_, impl Serialize>>,
        tx_builder: Option<TxBuilder<'_>>,
    ) -> Result<cosmrs::proto::cosmos::base::abci::v1beta1::TxResponse> {
        let msgs = multi_params
            .into_iter()
            .map(|params| {
                self.contract_execute_msg(params)
                    .and_then(|msg| msg_into_cosmrs_any(&msg))
            })
            .collect::<Result<Vec<_>>>()?;

        tx_builder
            .unwrap_or_else(|| self.tx_builder())
            .broadcast(msgs)
            .await
    }
}
