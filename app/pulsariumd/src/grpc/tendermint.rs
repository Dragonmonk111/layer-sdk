// use std::sync::Arc;

use pulsar_proto::tendermint::p2p::{DefaultNodeInfo, DefaultNodeInfoOther, ProtocolVersion};
use pulsar_proto::tendermint::types::{BlockId, Header, PartSetHeader};
use pulsar_proto::{
    cosmos::base::tendermint::v1beta1::{
        service_server::{Service, ServiceServer},
        AbciQueryRequest, AbciQueryResponse, GetBlockByHeightRequest, GetBlockByHeightResponse,
        GetLatestBlockRequest, GetLatestBlockResponse, GetLatestValidatorSetRequest,
        GetLatestValidatorSetResponse, GetNodeInfoRequest, GetNodeInfoResponse, GetSyncingRequest,
        GetSyncingResponse, GetValidatorSetByHeightRequest, GetValidatorSetByHeightResponse,
    },
    tendermint::types::Block,
};

use tendermint::node::info::TxIndexStatus;
use tendermint_rpc::{Client, HttpClient, Paging};

// auth::v1beta1::{
//     query_server::{Query, QueryServer},
//     QueryAccountRequest, QueryAccountResponse, QueryAccountsRequest, QueryAccountsResponse,
//     QueryModuleAccountByNameRequest, QueryModuleAccountByNameResponse, QueryParamsRequest,
//     QueryParamsResponse,
// };

// use pulsar_abci::MultiThreadedDispatcher;
// use tonic::{Request, Response, Status};

// use {abci_response_to_grpc, grpc_request_to_abci};

pub fn tendermint_service() -> ServiceServer<TendermintService> {
    ServiceServer::new(TendermintService::new())
}

pub struct TendermintService {
    client: HttpClient,
}

impl TendermintService {
    pub fn new() -> Self {
        // TODO: take as arg
        let target = "http://localhost:26657";
        Self {
            client: HttpClient::new(target).unwrap(),
        }
    }
}

#[tonic::async_trait]
impl Service for TendermintService {
    /// GetNodeInfo queries the current node info.
    async fn get_node_info(
        &self,
        _request: tonic::Request<GetNodeInfoRequest>,
    ) -> std::result::Result<tonic::Response<GetNodeInfoResponse>, tonic::Status> {
        // TODO: no unwrap, but proper errors
        let status = self.client.status().await.unwrap();
        let tx_index = match status.node_info.other.tx_index {
            TxIndexStatus::On => "on",
            TxIndexStatus::Off => "off",
        };
        let node_info = DefaultNodeInfo {
            protocol_version: Some(ProtocolVersion {
                p2p: status.node_info.protocol_version.p2p,
                block: status.node_info.protocol_version.block,
                app: status.node_info.protocol_version.app,
            }),
            default_node_id: status.node_info.id.to_string(),
            listen_addr: status.node_info.listen_addr.to_string(),
            network: status.node_info.network.to_string(),
            version: status.node_info.version.to_string(),
            channels: vec![], // ????
            moniker: status.node_info.moniker.to_string(),
            other: Some(DefaultNodeInfoOther {
                tx_index: tx_index.to_string(),
                rpc_address: status.node_info.other.rpc_address,
            }),
        };
        let response = GetNodeInfoResponse {
            default_node_info: Some(node_info),
            application_version: None,
        };
        Ok(tonic::Response::new(response))
    }

    /// GetSyncing queries node syncing.
    async fn get_syncing(
        &self,
        _request: tonic::Request<GetSyncingRequest>,
    ) -> std::result::Result<tonic::Response<GetSyncingResponse>, tonic::Status> {
        // TODO: no unwrap, but proper errors
        let status = self.client.status().await.unwrap();
        let response = GetSyncingResponse {
            syncing: status.sync_info.catching_up,
        };
        Ok(tonic::Response::new(response))
    }

    /// GetLatestBlock returns the latest block.
    async fn get_latest_block(
        &self,
        _request: tonic::Request<GetLatestBlockRequest>,
    ) -> std::result::Result<tonic::Response<GetLatestBlockResponse>, tonic::Status> {
        let block = self.client.latest_block().await.unwrap();
        let h = &block.block.header;
        let block_time: tendermint_proto::google::protobuf::Timestamp = h.time.clone().into();
        let block_data = Block {
            header: Some(Header {
                version: Some(pulsar_proto::tendermint::version::Consensus {
                    block: h.version.block,
                    app: h.version.app,
                }),
                chain_id: h.chain_id.to_string(),
                height: u64::from(h.height) as i64,
                time: Some(pulsar_proto::google::protobuf::Timestamp {
                    seconds: block_time.seconds,
                    nanos: block_time.nanos,
                }),
                last_block_id: None,
                last_commit_hash: opt_hash_to_vec(h.last_commit_hash),
                data_hash: opt_hash_to_vec(h.data_hash),
                validators_hash: hash_to_vec(h.validators_hash),
                next_validators_hash: hash_to_vec(h.next_validators_hash),
                consensus_hash: hash_to_vec(h.consensus_hash),
                app_hash: h.app_hash.as_bytes().into(),
                last_results_hash: opt_hash_to_vec(h.last_results_hash),
                evidence_hash: opt_hash_to_vec(h.evidence_hash),
                proposer_address: h.proposer_address.into(),
            }),
            data: Some(pulsar_proto::tendermint::types::Data {
                txs: block.block.data.clone(),
            }),
            evidence: None,
            last_commit: block.block.last_commit.as_ref().map(|c| {
                pulsar_proto::tendermint::types::Commit {
                    height: u64::from(c.height) as i64,
                    round: c.round.into(),
                    block_id: Some(BlockId {
                        hash: hash_to_vec(c.block_id.hash),
                        part_set_header: Some(PartSetHeader {
                            total: c.block_id.part_set_header.total,
                            hash: hash_to_vec(c.block_id.part_set_header.hash),
                        }),
                    }),
                    signatures: vec![],
                }
            }),
        };
        let response = GetLatestBlockResponse {
            block_id: None,
            block: Some(block_data),
            sdk_block: None,
        };
        Ok(tonic::Response::new(response))
    }

    /// GetBlockByHeight queries block for given height.
    async fn get_block_by_height(
        &self,
        request: tonic::Request<GetBlockByHeightRequest>,
    ) -> std::result::Result<tonic::Response<GetBlockByHeightResponse>, tonic::Status> {
        let _block = self
            .client
            .block(request.get_ref().height as u32)
            .await
            .unwrap();
        unimplemented!();
    }

    /// GetLatestValidatorSet queries latest validator-set.
    async fn get_latest_validator_set(
        &self,
        _request: tonic::Request<GetLatestValidatorSetRequest>,
    ) -> std::result::Result<tonic::Response<GetLatestValidatorSetResponse>, tonic::Status> {
        unimplemented!();
    }

    /// GetValidatorSetByHeight queries validator-set at a given height.
    async fn get_validator_set_by_height(
        &self,
        request: tonic::Request<GetValidatorSetByHeightRequest>,
    ) -> std::result::Result<tonic::Response<GetValidatorSetByHeightResponse>, tonic::Status> {
        let height = request.get_ref().height as u32;
        // let pagination = request.get_ref().pagination;
        let _validators = self
            .client
            .validators(height, Paging::Default)
            .await
            .unwrap();
        unimplemented!();
    }

    /// ABCIQuery defines a query handler that supports ABCI queries directly to the
    /// application, bypassing Tendermint completely. The ABCI query must contain
    /// a valid and supported path, including app, custom, p2p, and store.
    ///
    /// Since: cosmos-sdk 0.46
    async fn abci_query(
        &self,
        _request: tonic::Request<AbciQueryRequest>,
    ) -> std::result::Result<tonic::Response<AbciQueryResponse>, tonic::Status> {
        unimplemented!();
    }
}

fn opt_hash_to_vec(h: Option<tendermint::Hash>) -> Vec<u8> {
    match h {
        None => vec![],
        Some(v) => hash_to_vec(v),
    }
}

fn hash_to_vec(h: tendermint::Hash) -> Vec<u8> {
    match h {
        tendermint::Hash::None => vec![],
        tendermint::Hash::Sha256(v) => v.to_vec(),
    }
}
