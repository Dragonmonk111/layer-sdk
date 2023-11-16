// use std::sync::Arc;

use pulsar_proto::cosmos::base::tendermint::v1beta1::{
    service_server::{Service, ServiceServer},
    AbciQueryRequest, AbciQueryResponse, GetBlockByHeightRequest, GetBlockByHeightResponse,
    GetLatestBlockRequest, GetLatestBlockResponse, GetLatestValidatorSetRequest,
    GetLatestValidatorSetResponse, GetNodeInfoRequest, GetNodeInfoResponse, GetSyncingRequest,
    GetSyncingResponse, GetValidatorSetByHeightRequest, GetValidatorSetByHeightResponse,
};

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
    // TODO:
    target: String,
}

impl TendermintService {
    pub fn new() -> Self {
        Self {
            target: "http://localhost:26657".to_string(),
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
        unimplemented!();
    }

    /// GetSyncing queries node syncing.
    async fn get_syncing(
        &self,
        _request: tonic::Request<GetSyncingRequest>,
    ) -> std::result::Result<tonic::Response<GetSyncingResponse>, tonic::Status> {
        unimplemented!();
    }

    /// GetLatestBlock returns the latest block.
    async fn get_latest_block(
        &self,
        _request: tonic::Request<GetLatestBlockRequest>,
    ) -> std::result::Result<tonic::Response<GetLatestBlockResponse>, tonic::Status> {
        unimplemented!();
    }

    /// GetBlockByHeight queries block for given height.
    async fn get_block_by_height(
        &self,
        request: tonic::Request<GetBlockByHeightRequest>,
    ) -> std::result::Result<tonic::Response<GetBlockByHeightResponse>, tonic::Status> {
        unimplemented!();
    }

    /// GetLatestValidatorSet queries latest validator-set.
    async fn get_latest_validator_set(
        &self,
        request: tonic::Request<GetLatestValidatorSetRequest>,
    ) -> std::result::Result<tonic::Response<GetLatestValidatorSetResponse>, tonic::Status> {
        unimplemented!();
    }

    /// GetValidatorSetByHeight queries validator-set at a given height.
    async fn get_validator_set_by_height(
        &self,
        request: tonic::Request<GetValidatorSetByHeightRequest>,
    ) -> std::result::Result<tonic::Response<GetValidatorSetByHeightResponse>, tonic::Status> {
        unimplemented!();
    }

    /// ABCIQuery defines a query handler that supports ABCI queries directly to the
    /// application, bypassing Tendermint completely. The ABCI query must contain
    /// a valid and supported path, including app, custom, p2p, and store.
    ///
    /// Since: cosmos-sdk 0.46
    async fn abci_query(
        &self,
        request: tonic::Request<AbciQueryRequest>,
    ) -> std::result::Result<tonic::Response<AbciQueryResponse>, tonic::Status> {
        unimplemented!();
    }
}
