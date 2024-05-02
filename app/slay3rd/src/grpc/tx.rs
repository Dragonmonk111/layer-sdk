use std::sync::Arc;

use slay3r_proto::cosmos::tx::v1beta1::{
    service_server::{Service, ServiceServer},
    BroadcastTxRequest, BroadcastTxResponse, GetBlockWithTxsRequest, GetBlockWithTxsResponse,
    GetTxRequest, GetTxResponse, GetTxsEventRequest, GetTxsEventResponse, SimulateRequest,
    SimulateResponse, TxDecodeAminoRequest, TxDecodeAminoResponse, TxDecodeRequest,
    TxDecodeResponse, TxEncodeAminoRequest, TxEncodeAminoResponse, TxEncodeRequest,
    TxEncodeResponse,
};

use slay3r_abci::MultiThreadedDispatcher;
use tendermint_rpc::{Client, HttpClient};
use tonic::{Request, Response, Status};

use super::{abci_response_to_grpc, grpc_request_to_abci};

pub fn tx_service(
    dispatcher: Arc<MultiThreadedDispatcher>,
    rpc_url: &str,
) -> ServiceServer<TxService> {
    ServiceServer::new(TxService::new(dispatcher, rpc_url))
}

pub struct TxService {
    client: HttpClient,
    dispatcher: Arc<MultiThreadedDispatcher>,
}

impl TxService {
    pub fn new(dispatcher: Arc<MultiThreadedDispatcher>, rpc_url: &str) -> Self {
        Self {
            client: HttpClient::new(rpc_url).unwrap(),
            dispatcher,
        }
    }
}

#[tonic::async_trait]
impl Service for TxService {
    /// Simulate simulates executing a transaction for estimating gas usage.
    async fn simulate(
        &self,
        request: Request<SimulateRequest>,
    ) -> std::result::Result<tonic::Response<SimulateResponse>, Status> {
        println!("*** Got Simulate request ***");
        let query = grpc_request_to_abci("/cosmos.tx.v1beta1.Service/Simulate", request.get_ref());
        let response = self.dispatcher.dispatch_query(query).await;
        abci_response_to_grpc(response).map(Response::new)
    }

    /// GetTx fetches a tx by hash.
    async fn get_tx(
        &self,
        request: Request<GetTxRequest>,
    ) -> std::result::Result<tonic::Response<GetTxResponse>, Status> {
        // parse hex string into vec
        let binary = hex::decode(&request.get_ref().hash).unwrap();
        let hash = tendermint::hash::Hash::try_from(binary).unwrap(); // TODO: no unwrap, better error handling
                                                                      // TODO: what happens on missing tx hash? Error or empty response???
        let tx = self.client.tx(hash, false).await.unwrap();
        let response = GetTxResponse {
            tx: None, // TODO: decode into cosmos tx format
            tx_response: convert_tx_response(tx),
        };
        Ok(tonic::Response::new(response))
    }

    /// BroadcastTx broadcast transaction.
    async fn broadcast_tx(
        &self,
        _request: Request<BroadcastTxRequest>,
    ) -> std::result::Result<tonic::Response<BroadcastTxResponse>, Status> {
        todo!();
    }

    /// GetTxsEvent fetches txs by event.
    async fn get_txs_event(
        &self,
        _request: Request<GetTxsEventRequest>,
    ) -> std::result::Result<tonic::Response<GetTxsEventResponse>, Status> {
        todo!();
    }

    /// GetBlockWithTxs fetches a block with decoded txs.
    ///
    /// Since: cosmos-sdk 0.45.2
    async fn get_block_with_txs(
        &self,
        _request: Request<GetBlockWithTxsRequest>,
    ) -> std::result::Result<tonic::Response<GetBlockWithTxsResponse>, Status> {
        todo!();
    }

    /// TxDecode decodes the transaction.
    ///
    /// Since: cosmos-sdk 0.47
    async fn tx_decode(
        &self,
        _request: Request<TxDecodeRequest>,
    ) -> std::result::Result<tonic::Response<TxDecodeResponse>, Status> {
        Err(not_gonna_do_it("tx_decode"))
    }

    /// TxEncode encodes the transaction.
    ///
    /// Since: cosmos-sdk 0.47
    async fn tx_encode(
        &self,
        _request: Request<TxEncodeRequest>,
    ) -> std::result::Result<tonic::Response<TxEncodeResponse>, Status> {
        Err(not_gonna_do_it("tx_encode"))
    }

    /// TxEncodeAmino encodes an Amino transaction from JSON to encoded bytes.
    ///
    /// Since: cosmos-sdk 0.47
    async fn tx_encode_amino(
        &self,
        _request: Request<TxEncodeAminoRequest>,
    ) -> std::result::Result<tonic::Response<TxEncodeAminoResponse>, Status> {
        Err(not_gonna_do_it("tx_encode_amino"))
    }

    /// TxDecodeAmino decodes an Amino transaction from encoded bytes to JSON.
    ///
    /// Since: cosmos-sdk 0.47
    async fn tx_decode_amino(
        &self,
        _request: Request<TxDecodeAminoRequest>,
    ) -> std::result::Result<tonic::Response<TxDecodeAminoResponse>, Status> {
        Err(not_gonna_do_it("tx_decode_amino"))
    }
}

fn not_gonna_do_it(msg: &str) -> Status {
    Status::new(
        tonic::Code::Unimplemented,
        format!("We are not going to do it: {}", msg),
    )
}

// Return None only if empty
fn convert_tx_response(
    _tx: tendermint_rpc::endpoint::tx::Response,
) -> Option<slay3r_proto::cosmos::base::abci::v1beta1::TxResponse> {
    todo!()
}
