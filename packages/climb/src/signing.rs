pub mod contract;
pub mod ibc;
pub mod key;
pub mod middleware;

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
    vec,
};

use anyhow::{anyhow, Result};
use cosmrs::crypto::secp256k1::SigningKey;
use middleware::{SigningMiddlewareMapBody, SigningMiddlewareMapResp};

use super::TxBuilder;
use crate::{
    msg_into_cosmrs_any, querier::QueryClient, AddrString, ChainConfig, ChainId, SequenceStrategy,
    SequenceStrategyKind,
};

// Each combo of chain and seed phrase gets a single signing client
static SIGNING_CLIENT_CACHE: LazyLock<SigningClientCache> = LazyLock::new(SigningClientCache::new);

type CacheKey = (ChainId, AddrString);

struct SigningClientCache {
    clients: Mutex<HashMap<CacheKey, SigningClient>>,
}

impl SigningClientCache {
    fn new() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
        }
    }
}

// Cloning a SigningClient is pretty cheap
#[derive(Clone)]
pub struct SigningClient {
    pub querier: QueryClient,
    pub signing_key: Arc<SigningKey>,
    pub addr: AddrString,
    pub account_number: u64,
    /// Middleware to run before the tx is broadcast
    pub middleware_map_body: Arc<Vec<SigningMiddlewareMapBody>>,
    /// Middleware to run after the tx is broadcast
    pub middleware_map_resp: Arc<Vec<SigningMiddlewareMapResp>>,
    /// Strategy for determining the sequence number for txs
    /// not `pub` since changing it after the first call would be weird
    /// it will be applied when calling `tx_builder()`
    /// (i.e. it's always possible to manually construct a TxBuilder and override it)
    sequence_strategy: Arc<SequenceStrategy>,
}

impl SigningClient {
    /// if `sequence_strategy` is `None`, it will default to `Query`
    pub async fn new(
        chain_config: ChainConfig,
        sequence_strategy: Option<SequenceStrategy>,
        signing_key: SigningKey,
    ) -> Result<Self> {
        let addr =
            AddrString::new_pub_key(&signing_key.public_key(), chain_config.address_kind.clone())?;

        let client = {
            // keep lock in scope so it can be definitively dropped before the await
            let lock = SIGNING_CLIENT_CACHE.clients.lock().unwrap();
            lock.get(&(chain_config.chain_id.clone(), addr.clone()))
                .cloned()
        };

        match client {
            Some(client) => Ok(client),
            None => {
                let querier = QueryClient::new(chain_config.clone()).await?;

                let base_account = querier.base_account(&addr).await?;

                let sequence_strategy = Arc::new(
                    sequence_strategy.unwrap_or(SequenceStrategy::new(SequenceStrategyKind::Query)),
                );

                let mut _self = Self {
                    signing_key: Arc::new(signing_key),
                    querier,
                    addr,
                    account_number: base_account.account_number,
                    middleware_map_body: Arc::new(
                        middleware::SigningMiddlewareMapBody::default_list(),
                    ),
                    middleware_map_resp: Arc::new(
                        middleware::SigningMiddlewareMapResp::default_list(),
                    ),
                    sequence_strategy,
                };

                SIGNING_CLIENT_CACHE.clients.lock().unwrap().insert(
                    (chain_config.chain_id.clone(), _self.addr.clone()),
                    _self.clone(),
                );

                Ok(_self)
            }
        }
    }

    pub fn chain_id(&self) -> &ChainId {
        &self.querier.chain_config.chain_id
    }

    pub fn sequence_strategy_kind(&self) -> &SequenceStrategyKind {
        &self.sequence_strategy.kind
    }

    pub fn tx_builder(&self) -> TxBuilder<'_> {
        let mut tx_builder = TxBuilder::new(&self.querier, &self.signing_key);

        tx_builder
            .set_public_key(self.signing_key.public_key())
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
        recipient: AddrString,
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
        recipient: AddrString,
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
