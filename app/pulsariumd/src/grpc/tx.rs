use std::sync::Arc;

use pulsar_proto::cosmos::tx::v1beta1::{
    service_server::{Service, ServiceServer},
    BroadcastTxRequest, BroadcastTxResponse, GetBlockWithTxsRequest, GetBlockWithTxsResponse,
    GetTxRequest, GetTxResponse, GetTxsEventRequest, GetTxsEventResponse, SimulateRequest,
    SimulateResponse, TxDecodeAminoRequest, TxDecodeAminoResponse, TxDecodeRequest,
    TxDecodeResponse, TxEncodeAminoRequest, TxEncodeAminoResponse, TxEncodeRequest,
    TxEncodeResponse,
};

use pulsar_abci::MultiThreadedDispatcher;
use tonic::{Request, Response, Status};

use super::{abci_response_to_grpc, grpc_request_to_abci};

pub fn tx_service(dispatcher: Arc<MultiThreadedDispatcher>) -> ServiceServer<TxService> {
    ServiceServer::new(TxService::new(dispatcher))
}

pub struct TxService {
    dispatcher: Arc<MultiThreadedDispatcher>,
}

impl TxService {
    pub fn new(dispatcher: Arc<MultiThreadedDispatcher>) -> Self {
        Self { dispatcher }
    }
}

#[tonic::async_trait]
impl Service for TxService {
    /// Simulate simulates executing a transaction for estimating gas usage.
    async fn simulate(
        &self,
        request: tonic::Request<SimulateRequest>,
    ) -> std::result::Result<tonic::Response<SimulateResponse>, tonic::Status> {
        unimplemented!();
    }

    /// GetTx fetches a tx by hash.
    async fn get_tx(
        &self,
        request: tonic::Request<GetTxRequest>,
    ) -> std::result::Result<tonic::Response<GetTxResponse>, tonic::Status> {
        unimplemented!();
    }

    /// BroadcastTx broadcast transaction.
    async fn broadcast_tx(
        &self,
        request: tonic::Request<BroadcastTxRequest>,
    ) -> std::result::Result<tonic::Response<BroadcastTxResponse>, tonic::Status> {
        unimplemented!();
    }

    /// GetTxsEvent fetches txs by event.
    async fn get_txs_event(
        &self,
        request: tonic::Request<GetTxsEventRequest>,
    ) -> std::result::Result<tonic::Response<GetTxsEventResponse>, tonic::Status> {
        unimplemented!();
    }

    /// GetBlockWithTxs fetches a block with decoded txs.
    ///
    /// Since: cosmos-sdk 0.45.2
    async fn get_block_with_txs(
        &self,
        request: tonic::Request<GetBlockWithTxsRequest>,
    ) -> std::result::Result<tonic::Response<GetBlockWithTxsResponse>, tonic::Status> {
        unimplemented!();
    }

    /// TxDecode decodes the transaction.
    ///
    /// Since: cosmos-sdk 0.47
    async fn tx_decode(
        &self,
        request: tonic::Request<TxDecodeRequest>,
    ) -> std::result::Result<tonic::Response<TxDecodeResponse>, tonic::Status> {
        unimplemented!();
    }

    /// TxEncode encodes the transaction.
    ///
    /// Since: cosmos-sdk 0.47
    async fn tx_encode(
        &self,
        request: tonic::Request<TxEncodeRequest>,
    ) -> std::result::Result<tonic::Response<TxEncodeResponse>, tonic::Status> {
        unimplemented!();
    }

    /// TxEncodeAmino encodes an Amino transaction from JSON to encoded bytes.
    ///
    /// Since: cosmos-sdk 0.47
    async fn tx_encode_amino(
        &self,
        request: tonic::Request<TxEncodeAminoRequest>,
    ) -> std::result::Result<tonic::Response<TxEncodeAminoResponse>, tonic::Status> {
        unimplemented!();
    }

    /// TxDecodeAmino decodes an Amino transaction from encoded bytes to JSON.
    ///
    /// Since: cosmos-sdk 0.47
    async fn tx_decode_amino(
        &self,
        request: tonic::Request<TxDecodeAminoRequest>,
    ) -> std::result::Result<tonic::Response<TxDecodeAminoResponse>, tonic::Status> {
        unimplemented!();
    }
}
