use anyhow::{anyhow, Context, Result};
use cosmrs::proto::prost::Message;
use cosmwasm_std::Uint128;

use crate::AddrString;

use super::{QueryClient, QueryRequest};

impl QueryClient {
    pub async fn balance(&self, addr: AddrString, denom: Option<String>) -> Result<Option<u64>> {
        self.run_with_middleware(BalanceReq { addr, denom }).await
    }

    pub async fn all_balances(
        &self,
        addr: AddrString,
        limit_per_page: Option<u64>,
    ) -> Result<Vec<cosmwasm_std::Coin>> {
        self.run_with_middleware(AllBalancesReq {
            addr,
            limit_per_page,
        })
        .await
    }

    pub async fn base_account(
        &self,
        addr: &AddrString,
    ) -> Result<cosmrs::proto::cosmos::auth::v1beta1::BaseAccount> {
        self.run_with_middleware(BaseAccountReq { addr: addr.clone() })
            .await
    }
    pub async fn staking_params(&self) -> Result<cosmrs::proto::cosmos::staking::v1beta1::Params> {
        self.run_with_middleware(StakingParamsReq {}).await
    }
    pub async fn block(&self, height: Option<u64>) -> Result<BlockResp> {
        self.run_with_middleware(BlockReq { height }).await
    }
    pub async fn block_header(&self, height: Option<u64>) -> Result<BlockHeaderResp> {
        self.run_with_middleware(BlockHeaderReq { height }).await
    }
    pub async fn block_height(&self) -> Result<u64> {
        self.run_with_middleware(BlockHeightReq {}).await
    }
}

#[derive(Clone, Debug)]
pub struct BalanceReq {
    pub addr: AddrString,
    pub denom: Option<String>,
}

impl QueryRequest for BalanceReq {
    type QueryResponse = Option<u64>;

    async fn request(&self, client: QueryClient) -> Result<Self::QueryResponse> {
        let mut query_client = cosmrs::proto::cosmos::bank::v1beta1::query_client::QueryClient::new(
            client.grpc_channel.clone(),
        );

        let denom = self
            .denom
            .clone()
            .unwrap_or(client.chain_config.gas_denom.clone());

        let coin = query_client
            .balance(cosmrs::proto::cosmos::bank::v1beta1::QueryBalanceRequest {
                address: self.addr.to_string(),
                denom,
            })
            .await
            .map(|res| res.into_inner().balance)?;

        match coin {
            None => Ok(None),
            Some(coin) => {
                let amount = coin
                    .amount
                    .parse::<u64>()
                    .context("couldn't parse amount")?;
                Ok(Some(amount))
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct AllBalancesReq {
    pub addr: AddrString,
    pub limit_per_page: Option<u64>,
}

impl QueryRequest for AllBalancesReq {
    type QueryResponse = Vec<cosmwasm_std::Coin>;

    async fn request(&self, client: QueryClient) -> Result<Self::QueryResponse> {
        let mut query_client = cosmrs::proto::cosmos::bank::v1beta1::query_client::QueryClient::new(
            client.grpc_channel.clone(),
        );

        let mut coins = Vec::new();

        let mut pagination = None;

        let limit = self
            .limit_per_page
            .unwrap_or(client.balances_pagination_limit);

        loop {
            let resp = query_client
                .all_balances(
                    cosmrs::proto::cosmos::bank::v1beta1::QueryAllBalancesRequest {
                        address: self.addr.to_string(),
                        pagination,
                        resolve_denom: true,
                    },
                )
                .await
                .map(|res| res.into_inner())?;

            coins.extend(resp.balances.into_iter().map(|coin| {
                let amount = coin
                    .amount
                    .parse::<Uint128>()
                    .context("couldn't parse amount")
                    .unwrap();
                cosmwasm_std::Coin {
                    denom: coin.denom,
                    amount,
                }
            }));

            match &resp.pagination {
                None => break,
                Some(pagination_response) => {
                    if pagination_response.next_key.is_empty() {
                        break;
                    }
                }
            }

            pagination =
                resp.pagination.map(
                    |p| cosmrs::proto::cosmos::base::query::v1beta1::PageRequest {
                        key: p.next_key,
                        offset: 0,
                        limit,
                        count_total: false,
                        reverse: false,
                    },
                );
        }

        Ok(coins)
    }
}

#[derive(Clone, Debug)]
pub struct BaseAccountReq {
    pub addr: AddrString,
}

impl QueryRequest for BaseAccountReq {
    type QueryResponse = cosmrs::proto::cosmos::auth::v1beta1::BaseAccount;

    async fn request(&self, client: QueryClient) -> Result<Self::QueryResponse> {
        let mut query_client = cosmrs::proto::cosmos::auth::v1beta1::query_client::QueryClient::new(
            client.grpc_channel.clone(),
        );

        let account = query_client
            .account(cosmrs::proto::cosmos::auth::v1beta1::QueryAccountRequest {
                address: self.addr.to_string(),
            })
            .await
            .map(|res| res.into_inner().account)?
            .ok_or_else(|| anyhow!("account {} not found", self.addr))?;

        let account =
            cosmrs::proto::cosmos::auth::v1beta1::BaseAccount::decode(account.value.as_slice())
                .context("couldn't decode account")?;

        Ok(account)
    }
}

#[derive(Clone, Debug)]
pub struct StakingParamsReq {}

impl QueryRequest for StakingParamsReq {
    type QueryResponse = cosmrs::proto::cosmos::staking::v1beta1::Params;

    async fn request(
        &self,
        client: QueryClient,
    ) -> Result<cosmrs::proto::cosmos::staking::v1beta1::Params> {
        let mut query_client =
            cosmrs::proto::cosmos::staking::v1beta1::query_client::QueryClient::new(
                client.grpc_channel.clone(),
            );

        let resp = query_client
            .params(cosmrs::proto::cosmos::staking::v1beta1::QueryParamsRequest {})
            .await
            .map(|res| res.into_inner())
            .context("couldn't get staking params")?;

        resp.params.ok_or(anyhow!("no staking params found"))
    }
}

#[derive(Clone, Debug)]
pub struct BlockReq {
    pub height: Option<u64>,
}

#[derive(Debug)]
pub enum BlockResp {
    Sdk(cosmrs::proto::cosmos::base::tendermint::v1beta1::Block),
    Old(cosmrs::proto::tendermint::types::Block),
}

impl QueryRequest for BlockReq {
    type QueryResponse = BlockResp;

    async fn request(&self, client: QueryClient) -> Result<Self::QueryResponse> {
        let mut query_client =
            cosmrs::proto::cosmos::base::tendermint::v1beta1::service_client::ServiceClient::new(
                client.grpc_channel.clone(),
            );
        let height = self.height;

        match height {
            Some(height) => query_client
                .get_block_by_height(
                    cosmrs::proto::cosmos::base::tendermint::v1beta1::GetBlockByHeightRequest {
                        height: height.try_into()?,
                    },
                )
                .await
                .map_err(|err| err.into())
                .and_then(|res| {
                    let res = res.into_inner();
                    match res.sdk_block {
                        Some(block) => Ok(BlockResp::Sdk(block)),
                        None => res
                            .block
                            .map(BlockResp::Old)
                            .ok_or(anyhow!("no block found")),
                    }
                }),
            None => query_client
                .get_latest_block(
                    cosmrs::proto::cosmos::base::tendermint::v1beta1::GetLatestBlockRequest {},
                )
                .await
                .map_err(|err| err.into())
                .and_then(|res| {
                    let res = res.into_inner();
                    match res.sdk_block {
                        Some(block) => Ok(BlockResp::Sdk(block)),
                        None => res
                            .block
                            .map(BlockResp::Old)
                            .ok_or(anyhow!("no block found")),
                    }
                }),
        }
        .with_context(move || match height {
            Some(height) => format!("no block found at height {}", height),
            None => "no latest block found".to_string(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct BlockHeaderReq {
    pub height: Option<u64>,
}

#[derive(Debug)]
pub enum BlockHeaderResp {
    Sdk(cosmrs::proto::cosmos::base::tendermint::v1beta1::Header),
    Old(cosmrs::proto::tendermint::types::Header),
}

impl BlockHeaderResp {
    pub fn height(&self) -> Result<u64> {
        Ok(match self {
            BlockHeaderResp::Sdk(header) => header.height.try_into()?,
            BlockHeaderResp::Old(header) => header.height.try_into()?,
        })
    }

    pub fn time(&self) -> Option<tendermint_proto::google::protobuf::Timestamp> {
        match self {
            BlockHeaderResp::Sdk(header) => header.time,
            BlockHeaderResp::Old(header) => {
                header
                    .time
                    .map(|time| tendermint_proto::google::protobuf::Timestamp {
                        seconds: time.seconds,
                        nanos: time.nanos,
                    })
            }
        }
    }

    pub fn app_hash(&self) -> Vec<u8> {
        match self {
            BlockHeaderResp::Sdk(header) => header.app_hash.clone(),
            BlockHeaderResp::Old(header) => header.app_hash.clone(),
        }
    }

    pub fn next_validators_hash(&self) -> Vec<u8> {
        match self {
            BlockHeaderResp::Sdk(header) => header.next_validators_hash.clone(),
            BlockHeaderResp::Old(header) => header.next_validators_hash.clone(),
        }
    }
}

impl QueryRequest for BlockHeaderReq {
    type QueryResponse = BlockHeaderResp;

    async fn request(&self, client: QueryClient) -> Result<Self::QueryResponse> {
        let block = BlockReq {
            height: self.height,
        }
        .request(client)
        .await?;

        match block {
            BlockResp::Sdk(block) => Ok(BlockHeaderResp::Sdk(
                block.header.context("no header found")?,
            )),
            BlockResp::Old(block) => Ok(BlockHeaderResp::Old(
                block.header.context("no header found")?,
            )),
        }
    }
}

#[derive(Clone, Debug)]
pub struct BlockHeightReq {}

impl QueryRequest for BlockHeightReq {
    type QueryResponse = u64;

    async fn request(&self, client: QueryClient) -> Result<u64> {
        let header = BlockHeaderReq { height: None }.request(client).await?;

        Ok(match header {
            BlockHeaderResp::Sdk(header) => header.height,
            BlockHeaderResp::Old(header) => header.height,
        }
        .try_into()?)
    }
}
