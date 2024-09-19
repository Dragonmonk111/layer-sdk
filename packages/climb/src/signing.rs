pub mod contract;
pub mod ibc;
pub mod key;
pub mod middleware;

use std::sync::Arc;

use anyhow::{anyhow, Result};
use middleware::{SigningMiddlewareMapBody, SigningMiddlewareMapResp};

use super::TxBuilder;
use crate::{
    msg_into_cosmrs_any, querier::QueryClient, Address, ChainConfig, ChainId, SequenceStrategy,
    SequenceStrategyKind, TxSigner,
};

// Cloning a SigningClient is pretty cheap
#[derive(Clone)]
pub struct SigningClient {
    pub querier: QueryClient,
    pub signer: Arc<dyn TxSigner>,
    pub addr: Address,
    pub account_number: u64,
    /// Middleware to run before the tx is broadcast
    pub middleware_map_body: Arc<Vec<SigningMiddlewareMapBody>>,
    /// Middleware to run after the tx is broadcast
    pub middleware_map_resp: Arc<Vec<SigningMiddlewareMapResp>>,
    /// Strategy for determining the sequence number for txs
    /// it will be applied when calling `tx_builder()`
    /// (i.e. it's always possible to manually construct a TxBuilder and override it)
    /// Default is `SequenceStrategyKind::Query`
    pub sequence_strategy: SequenceStrategy,
}

impl SigningClient {
    pub async fn new(chain_config: ChainConfig, signer: impl TxSigner + 'static) -> Result<Self> {
        let addr = chain_config.address_from_pub_key(&signer.public_key())?;

        let querier = QueryClient::new(chain_config.clone()).await?;

        let base_account = querier.base_account(&addr).await?;

        Ok(Self {
            signer: Arc::new(signer),
            querier,
            addr,
            account_number: base_account.account_number,
            middleware_map_body: Arc::new(middleware::SigningMiddlewareMapBody::default_list()),
            middleware_map_resp: Arc::new(middleware::SigningMiddlewareMapResp::default_list()),
            sequence_strategy: SequenceStrategy::new(SequenceStrategyKind::Query),
        })
    }

    pub fn chain_id(&self) -> &ChainId {
        &self.querier.chain_config.chain_id
    }

    pub fn sequence_strategy_kind(&self) -> &SequenceStrategyKind {
        &self.sequence_strategy.kind
    }

    pub fn tx_builder(&self) -> TxBuilder<'_> {
        let mut tx_builder = TxBuilder::new(&self.querier, self.signer.as_ref());

        tx_builder
            .set_sender(self.addr.clone())
            .set_account_number(self.account_number)
            .set_sequence_strategy(self.sequence_strategy.clone());

        if self.middleware_map_body.len() > 0 {
            tx_builder.set_middleware_map_body(self.middleware_map_body.clone());
        }

        if self.middleware_map_resp.len() > 0 {
            tx_builder.set_middleware_map_resp(self.middleware_map_resp.clone());
        }

        tx_builder
    }

    pub async fn transfer(
        &self,
        denom: Option<String>,
        amount: u128,
        recipient: Address,
        tx_builder: Option<TxBuilder<'_>>,
    ) -> Result<cosmrs::proto::cosmos::base::abci::v1beta1::TxResponse> {
        tx_builder
            .unwrap_or_else(|| self.tx_builder())
            .broadcast([msg_into_cosmrs_any(
                &self.transfer_msg(denom, amount, recipient)?,
            )?])
            .await
    }

    pub fn transfer_msg(
        &self,
        denom: Option<String>,
        amount: u128,
        recipient: Address,
    ) -> Result<cosmrs::proto::cosmos::bank::v1beta1::MsgSend> {
        let denom = denom.unwrap_or(self.querier.chain_config.gas_denom.clone());

        let amount = cosmrs::proto::cosmos::base::v1beta1::Coin {
            amount: amount.to_string(),
            denom: denom.parse().map_err(|err| anyhow!("{}", err))?,
        };

        Ok(cosmrs::proto::cosmos::bank::v1beta1::MsgSend {
            from_address: self.addr.to_string(),
            to_address: recipient.to_string(),
            amount: vec![amount],
        })
    }
}
