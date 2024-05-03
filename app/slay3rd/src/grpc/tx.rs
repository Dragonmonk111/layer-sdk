use std::sync::Arc;

use cosmwasm_std::Binary;
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

pub(crate) fn invalid_arg(e: impl std::fmt::Display) -> Status {
    Status::new(tonic::Code::InvalidArgument, e.to_string())
}

pub(crate) fn gateway_error(e: tendermint_rpc::Error) -> Status {
    println!("Gateway Error: {:?}", e); // TODO: remove
    Status::new(tonic::Code::Internal, e.to_string())
}

impl TxService {
    async fn get_blocktime(
        &self,
        height: impl Into<tendermint::block::Height>,
    ) -> Result<String, Status> {
        let header = self
            .client
            .header(height.into())
            .await
            .map_err(gateway_error)?;
        Ok(header.header.time.to_rfc3339())
    }
}

#[tonic::async_trait]
impl Service for TxService {
    /// Simulate simulates executing a transaction for estimating gas usage.
    async fn simulate(
        &self,
        request: Request<SimulateRequest>,
    ) -> std::result::Result<tonic::Response<SimulateResponse>, Status> {
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
        let binary = hex::decode(&request.get_ref().hash).map_err(invalid_arg)?;
        let hash = tendermint::hash::Hash::try_from(binary).map_err(invalid_arg)?;
        // TODO: what happens on missing tx hash? Error or empty response???
        println!("Get Tx: {:?}", &hash); // TODO: remove

        let tx = self.client.tx(hash, false).await.map_err(gateway_error)?;
        let time = self.get_blocktime(tx.height).await?;

        let response = GetTxResponse {
            tx: None, // TODO: decode into cosmos tx format
            tx_response: Some(convert_tx_response(tx, time)),
        };
        println!("Response: {:?}", &response); // TODO: remove
        Ok(tonic::Response::new(response))
    }

    /// BroadcastTx broadcast transaction.
    async fn broadcast_tx(
        &self,
        request: Request<BroadcastTxRequest>,
    ) -> std::result::Result<tonic::Response<BroadcastTxResponse>, Status> {
        let tx_bytes = request.get_ref().tx_bytes.as_slice();
        let response = match request.get_ref().mode {
            2 => {
                // BroadcastMode::Sync
                let tx = self
                    .client
                    .broadcast_tx_sync(tx_bytes)
                    .await
                    .map_err(gateway_error)?;
                BroadcastTxResponse {
                    tx_response: Some(convert_tx_broadcast_response(
                        tx.codespace,
                        tx.code,
                        tx.data,
                        tx.log,
                        tx.hash,
                    )),
                }
            }
            3 => {
                // BroadcastMode::Async
                let tx = self
                    .client
                    .broadcast_tx_async(tx_bytes)
                    .await
                    .map_err(gateway_error)?;
                BroadcastTxResponse {
                    tx_response: Some(convert_tx_broadcast_response(
                        tx.codespace,
                        tx.code,
                        tx.data,
                        tx.log,
                        tx.hash,
                    )),
                }
            }
            _ => {
                return Err(Status::new(
                    tonic::Code::InvalidArgument,
                    "Invalid broadcast mode",
                ));
            }
        };
        println!(
            "BroadcastTx: {:?}",
            &response.tx_response.as_ref().unwrap().txhash
        ); // TODO: remove
        Ok(tonic::Response::new(response))
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

// Reads the responses from blocks
fn convert_tx_response(
    tx: tendermint_rpc::endpoint::tx::Response,
    timestamp: String, // This  must be queried separately from tendermint rpc
) -> slay3r_proto::cosmos::base::abci::v1beta1::TxResponse {
    let exec_tx = tx.tx_result;

    slay3r_proto::cosmos::base::abci::v1beta1::TxResponse {
        height: tx.height.into(),
        txhash: hex::encode(&tx.hash),
        codespace: exec_tx.codespace,
        code: exec_tx.code.into(),
        data: Binary::new(exec_tx.data.into()).to_base64(),
        raw_log: exec_tx.log,
        logs: vec![], // TODO: parse raw_logs???
        info: exec_tx.info,
        gas_wanted: exec_tx.gas_wanted,
        gas_used: exec_tx.gas_used,
        tx: None, // TODO
        timestamp,
        events: exec_tx.events.into_iter().map(convert_event).collect(),
    }
}

fn convert_event(event: tendermint::abci::Event) -> slay3r_proto::tendermint::abci::Event {
    slay3r_proto::tendermint::abci::Event {
        r#type: event.kind,
        attributes: event
            .attributes
            .iter()
            .map(|attr| slay3r_proto::tendermint::abci::EventAttribute {
                key: attr.key_str().unwrap().to_string(),
                value: attr.value_str().unwrap().to_string(),
                index: attr.index(),
            })
            .collect(),
    }
}

// Response from broadcast sync call
// Return None only if empty
fn convert_tx_broadcast_response(
    codespace: String,
    code: tendermint::abci::Code,
    data: bytes::Bytes,
    log: String,
    hash: tendermint::hash::Hash,
) -> slay3r_proto::cosmos::base::abci::v1beta1::TxResponse {
    slay3r_proto::cosmos::base::abci::v1beta1::TxResponse {
        height: 0,
        // hex encoding
        txhash: hex::encode(&hash),
        codespace: codespace,
        code: code.into(),
        // base64 encoding
        data: Binary::new(data.into()).to_base64(),
        raw_log: log,
        logs: vec![],
        info: "".to_string(),
        gas_wanted: 0,
        gas_used: 0,
        tx: None,
        timestamp: "".to_string(),
        events: vec![],
    }
}
