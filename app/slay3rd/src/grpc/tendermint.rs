// use std::sync::Arc;

use std::sync::Arc;

use slay3r_abci::MultiThreadedDispatcher;
use slay3r_proto::cosmos::base::tendermint::v1beta1::VersionInfo;
use slay3r_proto::tendermint::p2p::{DefaultNodeInfo, DefaultNodeInfoOther, ProtocolVersion};
use slay3r_proto::tendermint::types::{BlockId, Header, PartSetHeader};
use slay3r_proto::{
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

use crate::grpc::{abci_response_to_grpc, grpc_request_to_abci};

use super::tx::gateway_error;

pub fn tendermint_service(
    dispatcher: Arc<MultiThreadedDispatcher>,
    rpc_url: &str,
) -> ServiceServer<TendermintService> {
    ServiceServer::new(TendermintService::new(dispatcher, rpc_url))
}

pub struct TendermintService {
    client: HttpClient,
    dispatcher: Arc<MultiThreadedDispatcher>,
}

impl TendermintService {
    pub fn new(dispatcher: Arc<MultiThreadedDispatcher>, rpc_url: &str) -> Self {
        Self {
            client: HttpClient::new(rpc_url).unwrap(),
            dispatcher,
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
        let status = self.client.status().await.map_err(gateway_error)?;
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
            channels: status.node_info.channels.to_string().into_bytes(),
            moniker: status.node_info.moniker.to_string(),
            other: Some(DefaultNodeInfoOther {
                tx_index: tx_index.to_string(),
                rpc_address: status.node_info.other.rpc_address,
            }),
        };
        let version_info = VersionInfo {
            name: "Slay3r".to_string(),
            app_name: "Slay3r".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            git_commit: env!("GIT_HASH").to_string(),
            build_tags: "".to_string(), // FIXME: read features
            go_version: env!("RUST_VERSION").to_string(),
            build_deps: vec![], // FIXME: what deps to put here?
            // Pretend we are 0.50.0 for now
            cosmos_sdk_version: "0.50.0".to_string(),
        };
        let response = GetNodeInfoResponse {
            default_node_info: Some(node_info),
            application_version: Some(version_info),
        };
        Ok(tonic::Response::new(response))
    }

    /// GetSyncing queries node syncing.
    async fn get_syncing(
        &self,
        _request: tonic::Request<GetSyncingRequest>,
    ) -> std::result::Result<tonic::Response<GetSyncingResponse>, tonic::Status> {
        let status = self.client.status().await.map_err(gateway_error)?;
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
        let block = self.client.latest_block().await.map_err(gateway_error)?;
        let block_data = convert_tendermint_block(&block.block);
        let response = GetLatestBlockResponse {
            block_id: Some(convert_block_id(&block.block_id)),
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
        let block = self
            .client
            .block(request.get_ref().height as u32)
            .await
            .map_err(gateway_error)?;
        let block_data = convert_tendermint_block(&block.block);
        let response = GetBlockByHeightResponse {
            block_id: Some(convert_block_id(&block.block_id)),
            block: Some(block_data),
            sdk_block: None,
        };
        Ok(tonic::Response::new(response))
    }

    /// GetLatestValidatorSet queries latest validator-set.
    async fn get_latest_validator_set(
        &self,
        _request: tonic::Request<GetLatestValidatorSetRequest>,
    ) -> std::result::Result<tonic::Response<GetLatestValidatorSetResponse>, tonic::Status> {
        todo!();
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
            .map_err(gateway_error)?;
        todo!();
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
        println!("*** Got ABCI query request ***");
        let query = grpc_request_to_abci(&request.get_ref().path, &request.get_ref().data);
        let response = self.dispatcher.dispatch_query(query).await;
        abci_response_to_grpc(response).map(tonic::Response::new)
    }
}

pub(crate) fn convert_tendermint_block(
    b: &tendermint::block::Block,
) -> slay3r_proto::tendermint::types::Block {
    Block {
        header: Some(convert_tendermint_header(&b.header)),
        data: Some(slay3r_proto::tendermint::types::Data {
            txs: b.data.clone(),
        }),
        evidence: None, // TODO
        last_commit: b.last_commit.as_ref().map(convert_tendermint_commit),
    }
}

fn convert_tendermint_header(
    h: &tendermint::block::Header,
) -> slay3r_proto::tendermint::types::Header {
    let block_time: tendermint_proto::google::protobuf::Timestamp = h.time.into();
    Header {
        version: Some(slay3r_proto::tendermint::version::Consensus {
            block: h.version.block,
            app: h.version.app,
        }),
        chain_id: h.chain_id.to_string(),
        height: u64::from(h.height) as i64,
        time: Some(slay3r_proto::google::protobuf::Timestamp {
            seconds: block_time.seconds,
            nanos: block_time.nanos,
        }),
        last_block_id: None, // TODO
        last_commit_hash: opt_hash_to_vec(h.last_commit_hash),
        data_hash: opt_hash_to_vec(h.data_hash),
        validators_hash: hash_to_vec(h.validators_hash),
        next_validators_hash: hash_to_vec(h.next_validators_hash),
        consensus_hash: hash_to_vec(h.consensus_hash),
        app_hash: h.app_hash.as_bytes().into(),
        last_results_hash: opt_hash_to_vec(h.last_results_hash),
        evidence_hash: opt_hash_to_vec(h.evidence_hash),
        proposer_address: h.proposer_address.into(),
    }
}

fn convert_tendermint_commit(
    c: &tendermint::block::Commit,
) -> slay3r_proto::tendermint::types::Commit {
    slay3r_proto::tendermint::types::Commit {
        height: u64::from(c.height) as i64,
        round: c.round.into(),
        block_id: Some(convert_block_id(&c.block_id)),
        signatures: vec![], // TODO
    }
}

pub(crate) fn convert_block_id(id: &tendermint::block::Id) -> BlockId {
    BlockId {
        hash: hash_to_vec(id.hash),
        part_set_header: Some(PartSetHeader {
            total: id.part_set_header.total,
            hash: hash_to_vec(id.part_set_header.hash),
        }),
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
