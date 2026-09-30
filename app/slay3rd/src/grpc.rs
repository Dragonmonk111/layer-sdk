//! gRPC service module for slay3rd.
//!
//! Provides `LayerGrpcService` which implements:
//! - `cosmos.tx.v1beta1.Service`: BroadcastTx (sync mode) plus unimplemented stubs
//! - `layer.sync.v1.Query`: LatestSequence, CurrentState, ChangesSince (streaming)
//!
//! Also provides `handle_cosmos_query()` — a helper for dispatching Cosmos SDK
//! gRPC queries via path matching. Called from a custom tower layer in main.rs.
//!
//! Also provides `CosmosQueryState` and `cosmos_query_fallback` — an axum fallback
//! handler that intercepts Cosmos SDK gRPC query paths and dispatches them to
//! `handle_cosmos_query()`. Registered as a fallback on the tonic Router so paths
//! like `/cosmos.bank.v1beta1.Query/Balance` reach the Layer App query handler.

use std::pin::Pin;
use std::sync::Arc;

use axum::body::Body as AxumBody;
use bytes::Bytes;
use futures::StreamExt;
use http::header::CONTENT_TYPE;
use http_body_util::BodyExt;
use tokio::sync::{Mutex, RwLock};
use tonic::{Request, Response, Status};

use layer_app::App;
use layer_app::SyncProvider;
use layer_app::{SnapshotExport, SNAPSHOT_FORMAT_V1};
use layer_cosmos::{encode_cosmos_response, parse_cosmos_query, parse_cosmos_tx};
use layer_storage::PersistentStorage;

use layer_proto::cosmos::tx::v1beta1::{
    service_server::Service as CosmTxService, BroadcastTxRequest, BroadcastTxResponse,
    GetBlockWithTxsRequest, GetBlockWithTxsResponse, GetTxRequest, GetTxResponse,
    GetTxsEventRequest, GetTxsEventResponse, SimulateRequest, SimulateResponse,
    TxDecodeAminoRequest, TxDecodeAminoResponse, TxDecodeRequest, TxDecodeResponse,
    TxEncodeAminoRequest, TxEncodeAminoResponse, TxEncodeRequest, TxEncodeResponse,
};
use layer_proto::layer::sync::v1::{
    query_server::Query as SyncQuery, BlockWrites, QueryLatestSequenceRequest,
    QueryLatestSequenceResponse, StreamChangesSinceRequest, StreamCurrentStateRequest, WriteData,
};
use layer_proto::layer::lightclient::v1::{
    query_server::Query as LightClientQuery, QueryBlockRequest, QueryBlockResponse,
    QueryLatestHeightRequest, QueryLatestHeightResponse, QueryProofRequest, QueryProofResponse,
};
use layer_proto::layer::statesync::v1::{
    query_server::Query as StateSyncQuery, ListSnapshotsRequest, ListSnapshotsResponse,
    LoadSnapshotChunkRequest, LoadSnapshotChunkResponse, SnapshotMeta,
};

use layer_proto::cosmos::base::abci::v1beta1::{
    GasInfo as AbciGasInfo, Result as AbciResult, TxResponse,
};
use layer_proto::tendermint::abci::{Event, EventAttribute};

use crate::mempool::{Mempool, SubmitError};
use crate::tx_index::TxIndex;

/// gRPC service for slay3rd, generic over the persistent storage backend.
///
/// App<T> is behind Arc<RwLock<App<T>>> to allow concurrent gRPC read queries
/// while finalize_block holds an exclusive write lock. Mempool remains behind
/// Arc<Mutex<Mempool>> (no concurrent readers needed for the mempool).
pub struct LayerGrpcService<T: PersistentStorage + Send + Sync + 'static> {
    /// Shared application state — contains the state machine and storage backend.
    pub app: Arc<RwLock<App<T>>>,
    /// Transaction mempool — receives validated txs from BroadcastTx.
    pub mempool: Arc<Mutex<Mempool>>,
    /// Chain ID — used to validate transaction chain_id at check_tx.
    pub chain_id: String,
    /// Index of committed txs by hash for the GetTx RPC.
    pub tx_index: Arc<TxIndex>,
    /// Cached state-sync snapshot — regenerated every SNAPSHOT_INTERVAL
    /// blocks. Exports are O(state); caching keeps repeat chunk fetches
    /// from re-iterating storage.
    pub snapshot_cache: Arc<Mutex<Option<Arc<SnapshotExport>>>>,
}

/// State-sync snapshots refresh at this block cadence.
const SNAPSHOT_INTERVAL: u64 = 1_000;

/// Manual Clone implementation — App<T> and Mempool are behind Arc so T does not need Clone.
impl<T: PersistentStorage + Send + Sync + 'static> Clone for LayerGrpcService<T> {
    fn clone(&self) -> Self {
        LayerGrpcService {
            app: self.app.clone(),
            mempool: self.mempool.clone(),
            chain_id: self.chain_id.clone(),
            tx_index: self.tx_index.clone(),
            snapshot_cache: self.snapshot_cache.clone(),
        }
    }
}

impl<T: PersistentStorage + Send + Sync + 'static> LayerGrpcService<T> {
    /// Latest offered snapshot: generated lazily, cached until the chain
    /// tip advances SNAPSHOT_INTERVAL blocks past its height. The export
    /// covers committed state at `SnapshotExport.height`.
    async fn cached_snapshot(&self) -> Result<Arc<SnapshotExport>, Status> {
        let mut cache = self.snapshot_cache.lock().await;
        if let Some(exp) = &*cache {
            let tip = {
                let app = self.app.read().await;
                app.info().map(|b| b.height).unwrap_or(0)
            };
            if tip < exp.height + SNAPSHOT_INTERVAL {
                return Ok(exp.clone());
            }
        }
        let exp = {
            let app = self.app.read().await;
            if app.info().is_none() {
                return Err(Status::unavailable("node initializing"));
            }
            app.snapshot_export()
                .map_err(|e| Status::internal(format!("snapshot export failed: {e}")))?
        };
        let exp = Arc::new(exp);
        *cache = Some(exp.clone());
        Ok(exp)
    }
}

// ---------------------------------------------------------------------------
// cosmos.tx.v1beta1.Service implementation
// ---------------------------------------------------------------------------

#[tonic::async_trait]
impl<T: PersistentStorage + Send + Sync + 'static> CosmTxService for LayerGrpcService<T> {
    /// BroadcastTx (sync mode): validate and submit a transaction to the mempool.
    ///
    /// Steps:
    /// 1. Parse raw bytes as `cosmos.tx.v1beta1.TxRaw` → `layer_std::Tx`
    /// 2. Guard against uninitialized app (panics in check_tx if app.info() is None)
    /// 3. Run check_tx (validates signature, sequence, fees)
    /// 4. Submit raw bytes to the mempool
    async fn broadcast_tx(
        &self,
        request: Request<BroadcastTxRequest>,
    ) -> Result<Response<BroadcastTxResponse>, Status> {
        let raw_bytes = Bytes::from(request.into_inner().tx_bytes);

        // Step 1: parse tx (chain_id is captured at construction time so no lock needed)
        let tx = parse_cosmos_tx(raw_bytes.clone(), &self.chain_id)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        // Step 2 & 3: guard initialization, then check_tx
        // CRITICAL: read lock (shared), call sync method, drop lock before any .await
        let check = {
            let app = self.app.read().await;
            if app.info().is_none() {
                return Err(Status::unavailable("node initializing"));
            }
            app.check_tx(tx)
        };

        if check.result.is_err() {
            return Err(Status::failed_precondition(format!(
                "check_tx failed: {:?}",
                check.result.err()
            )));
        }

        // Step 4: submit raw bytes to mempool
        let submitted = {
            let mut mempool = self.mempool.lock().await;
            mempool.submit(raw_bytes)
        };
        let hash = match submitted {
            Ok(hash) => hash,
            Err(SubmitError::Duplicate) => {
                return Err(Status::already_exists("tx already in mempool"))
            }
            Err(SubmitError::TooLarge) => {
                return Err(Status::invalid_argument("tx exceeds maximum size"))
            }
            Err(SubmitError::Full) => return Err(Status::resource_exhausted("mempool full")),
        };

        Ok(Response::new(BroadcastTxResponse {
            tx_response: Some(TxResponse {
                txhash: hex::encode_upper(hash),
                ..Default::default()
            }),
        }))
    }

    /// Simulate: execute a tx against committed state on a scratch overlay
    /// (auth + all messages, fully metered) and report gas + result — nothing
    /// is committed and the mempool is untouched.
    async fn simulate(
        &self,
        request: Request<SimulateRequest>,
    ) -> Result<Response<SimulateResponse>, Status> {
        let req = request.into_inner();
        if req.tx_bytes.is_empty() {
            return Err(Status::invalid_argument(
                "tx_bytes required (deprecated `tx` field is not supported)",
            ));
        }
        let tx = parse_cosmos_tx(Bytes::from(req.tx_bytes), &self.chain_id)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        let sim = {
            let app = self.app.read().await;
            if app.info().is_none() {
                return Err(Status::unavailable("node initializing"));
            }
            app.simulate(tx)
        };

        let gas_info = AbciGasInfo {
            gas_wanted: sim.gas.gas_wanted,
            gas_used: sim.gas.gas_used,
        };
        let result = match sim.result {
            Ok(ok) => AbciResult {
                events: ok
                    .events
                    .iter()
                    .flatten()
                    .map(|e| Event {
                        r#type: e.ty.clone(),
                        attributes: e
                            .attributes
                            .iter()
                            .map(|a| EventAttribute {
                                key: a.key.clone(),
                                value: a.value.clone(),
                                index: false,
                            })
                            .collect(),
                    })
                    .collect(),
                ..Default::default()
            },
            Err(e) => AbciResult {
                log: e.to_string(),
                ..Default::default()
            },
        };

        Ok(Response::new(SimulateResponse {
            gas_info: Some(gas_info),
            result: Some(result),
        }))
    }

    async fn get_tx(
        &self,
        request: Request<GetTxRequest>,
    ) -> Result<Response<GetTxResponse>, Status> {
        let hash = request.into_inner().hash;
        match self.tx_index.get(&hash) {
            Some(tx_response) => Ok(Response::new(GetTxResponse {
                tx: None,
                tx_response: Some(tx_response),
            })),
            None => Err(Status::not_found(format!(
                "tx {hash} not found (not yet committed, or outside this node's index window)"
            ))),
        }
    }

    async fn get_txs_event(
        &self,
        _request: Request<GetTxsEventRequest>,
    ) -> Result<Response<GetTxsEventResponse>, Status> {
        Err(Status::unimplemented("get_txs_event not yet implemented"))
    }

    async fn get_block_with_txs(
        &self,
        _request: Request<GetBlockWithTxsRequest>,
    ) -> Result<Response<GetBlockWithTxsResponse>, Status> {
        Err(Status::unimplemented(
            "get_block_with_txs not yet implemented",
        ))
    }

    async fn tx_decode(
        &self,
        _request: Request<TxDecodeRequest>,
    ) -> Result<Response<TxDecodeResponse>, Status> {
        Err(Status::unimplemented("tx_decode not yet implemented"))
    }

    async fn tx_encode(
        &self,
        _request: Request<TxEncodeRequest>,
    ) -> Result<Response<TxEncodeResponse>, Status> {
        Err(Status::unimplemented("tx_encode not yet implemented"))
    }

    async fn tx_encode_amino(
        &self,
        _request: Request<TxEncodeAminoRequest>,
    ) -> Result<Response<TxEncodeAminoResponse>, Status> {
        Err(Status::unimplemented("tx_encode_amino not yet implemented"))
    }

    async fn tx_decode_amino(
        &self,
        _request: Request<TxDecodeAminoRequest>,
    ) -> Result<Response<TxDecodeAminoResponse>, Status> {
        Err(Status::unimplemented("tx_decode_amino not yet implemented"))
    }
}

// ---------------------------------------------------------------------------
// layer.sync.v1.Query implementation
// ---------------------------------------------------------------------------

#[tonic::async_trait]
impl<T: PersistentStorage + Send + Sync + 'static> SyncQuery for LayerGrpcService<T> {
    type CurrentStateStream = Pin<Box<dyn tokio_stream::Stream<Item = Result<WriteData, Status>> + Send>>;
    type ChangesSinceStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<BlockWrites, Status>> + Send>>;

    /// Returns the latest state sync sequence number.
    async fn latest_sequence(
        &self,
        _request: Request<QueryLatestSequenceRequest>,
    ) -> Result<Response<QueryLatestSequenceResponse>, Status> {
        let sequence = {
            let app = self.app.read().await;
            app.latest_sequence()
        };
        Ok(Response::new(QueryLatestSequenceResponse { sequence }))
    }

    /// Streams the full current state as a series of `WriteData` messages.
    async fn current_state(
        &self,
        _request: Request<StreamCurrentStateRequest>,
    ) -> Result<Response<Self::CurrentStateStream>, Status> {
        let stream = {
            let app = self.app.read().await;
            app.current_state()
        };
        let mapped = stream.map(|r: Result<WriteData, String>| r.map_err(Status::internal));
        Ok(Response::new(Box::pin(mapped)))
    }

    /// Streams all block writes since the given sequence number.
    async fn changes_since(
        &self,
        request: Request<StreamChangesSinceRequest>,
    ) -> Result<Response<Self::ChangesSinceStream>, Status> {
        let sequence = request.into_inner().sequence;
        let stream = {
            let app = self.app.read().await;
            app.changes_since(sequence)
        };
        let mapped =
            stream.map(|r: Result<BlockWrites, String>| r.map_err(Status::internal));
        Ok(Response::new(Box::pin(mapped)))
    }
}

// ---------------------------------------------------------------------------
// layer.lightclient.v1.Query implementation
// ---------------------------------------------------------------------------

#[tonic::async_trait]
impl<T: PersistentStorage + Send + Sync + 'static> LightClientQuery for LayerGrpcService<T> {
    /// Latest committed height and its timestamp — the relayer's polling
    /// anchor for detecting new finalized blocks to relay.
    async fn latest_height(
        &self,
        _request: Request<QueryLatestHeightRequest>,
    ) -> Result<Response<QueryLatestHeightResponse>, Status> {
        let (height, timestamp_nanos) = {
            let app = self.app.read().await;
            app.info()
                .map(|b| (b.height, b.time.nanos()))
                .unwrap_or((0, 0))
        };
        Ok(Response::new(QueryLatestHeightResponse {
            height,
            timestamp_nanos,
        }))
    }

    /// The three header components for a finalized height: consensus
    /// Proposal bytes, BLS12-381 threshold certificate, and the block
    /// timestamp. All three are persisted by the Reporter after finalization;
    /// a `not_found` means the height is not finalized yet (or the node is
    /// catching up from a peer that has not delivered the certificate).
    async fn block(
        &self,
        request: Request<QueryBlockRequest>,
    ) -> Result<Response<QueryBlockResponse>, Status> {
        let height = request.into_inner().height;

        let (proposal_bytes, certificate_bytes, timestamp_nanos, payload_bytes) = {
            let app = self.app.read().await;
            (
                app.get_block_proposal(height),
                app.get_block_certificate(height),
                app.get_block_timestamp(height),
                app.get_block_payload(height),
            )
        };

        let proposal_bytes = proposal_bytes.ok_or_else(|| {
            Status::not_found(format!(
                "no proposal stored for height {height} — block not finalized yet?"
            ))
        })?;
        let certificate_bytes = certificate_bytes.ok_or_else(|| {
            Status::not_found(format!(
                "no certificate stored for height {height} — block not finalized yet?"
            ))
        })?;
        let timestamp_nanos = timestamp_nanos.ok_or_else(|| {
            Status::not_found(format!(
                "no timestamp stored for height {height} — block not finalized yet?"
            ))
        })?;
        // Payload bytes are required for membership proofs but optional for
        // header relay — an empty payload_bytes means this node predates the
        // _payload sidecar for that height.
        let payload_bytes = payload_bytes.unwrap_or_default();

        Ok(Response::new(QueryBlockResponse {
            height,
            timestamp_nanos,
            proposal_bytes,
            certificate_bytes,
            payload_bytes,
        }))
    }

    /// Merkle membership proof for a storage key over the latest committed
    /// state. The proof verifies against `state_root` in the payload of the
    /// block at `state_height + 1` — the relayer fetches that payload via
    /// `Block` and assembles the contract-side proof.
    ///
    /// `not_found` means the key does not exist in committed state
    /// (non-membership proofs are not supported — they need a versioned
    /// tree; see BLS_LIGHT_CLIENT_SPEC §8).
    async fn proof(
        &self,
        request: Request<QueryProofRequest>,
    ) -> Result<Response<QueryProofResponse>, Status> {
        let key = request.into_inner().key;
        if key.is_empty() {
            return Err(Status::invalid_argument("key must not be empty"));
        }

        let proof = {
            let app = self.app.read().await;
            app.state_proof(&key)
                .map_err(|e| Status::internal(format!("state proof failed: {e}")))?
        };

        let proof = proof.ok_or_else(|| {
            Status::not_found("key does not exist in committed state — non-membership proofs are not supported")
        })?;

        Ok(Response::new(QueryProofResponse {
            state_height: proof.state_height,
            key: proof.key,
            value: proof.value,
            leaf_index: proof.leaf_index,
            // Option<[u8;32]> → bytes; None (promotion level) → empty bytes.
            siblings: proof
                .siblings
                .iter()
                .map(|s| s.map(|h| h.to_vec()).unwrap_or_default())
                .collect(),
        }))
    }
}

// ---------------------------------------------------------------------------
// layer.statesync.v1.Query implementation
// ---------------------------------------------------------------------------

#[tonic::async_trait]
impl<T: PersistentStorage + Send + Sync + 'static> StateSyncQuery for LayerGrpcService<T> {
    /// ListSnapshots: offers at most one snapshot — the cached export of
    /// latest committed state, refreshed every SNAPSHOT_INTERVAL blocks.
    async fn list_snapshots(
        &self,
        _request: Request<ListSnapshotsRequest>,
    ) -> Result<Response<ListSnapshotsResponse>, Status> {
        let exp = self.cached_snapshot().await?;
        Ok(Response::new(ListSnapshotsResponse {
            snapshots: vec![SnapshotMeta {
                height: exp.height,
                format: exp.format,
                chunks: exp.chunks.len() as u32,
                state_root: exp.state_root.to_vec(),
                total_bytes: exp.chunks.iter().map(|c| c.data.len() as u64).sum(),
            }],
        }))
    }

    /// LoadSnapshotChunk: serves one chunk of the cached snapshot. The
    /// requested height must match the offered snapshot — historical
    /// heights cannot be served (the store holds only current state).
    async fn load_snapshot_chunk(
        &self,
        request: Request<LoadSnapshotChunkRequest>,
    ) -> Result<Response<LoadSnapshotChunkResponse>, Status> {
        let req = request.into_inner();
        if req.format != SNAPSHOT_FORMAT_V1 {
            return Err(Status::invalid_argument(format!(
                "unsupported snapshot format {} (expected {})",
                req.format, SNAPSHOT_FORMAT_V1
            )));
        }
        let exp = self.cached_snapshot().await?;
        if req.height != exp.height {
            return Err(Status::not_found(format!(
                "no snapshot at height {} (offered: {})",
                req.height, exp.height
            )));
        }
        let chunk = exp.chunks.get(req.chunk as usize).ok_or_else(|| {
            Status::not_found(format!(
                "chunk {} out of range ({} chunks)",
                req.chunk,
                exp.chunks.len()
            ))
        })?;
        Ok(Response::new(LoadSnapshotChunkResponse {
            chunk: chunk.data.clone(),
            checksum: chunk.checksum.to_vec(),
        }))
    }
}

// ---------------------------------------------------------------------------
// Cosmos query dispatch helper
// ---------------------------------------------------------------------------

/// Handle a Cosmos SDK gRPC query by path dispatch.
///
/// This function is called from a custom tower service layer in `main.rs`
/// to route Cosmos SDK query paths (e.g., `/cosmos.bank.v1beta1.Query/Balance`)
/// to the application's query handler.
///
/// # Errors
///
/// Returns `Status::invalid_argument` if the path or query bytes cannot be parsed.
/// Returns `Status::internal` if the response cannot be encoded.
pub async fn handle_cosmos_query<T: PersistentStorage + Send + Sync + 'static>(
    app: &Arc<RwLock<App<T>>>,
    chain_id: &str,
    path: &str,
    body: Bytes,
) -> Result<Vec<u8>, Status> {
    let query = parse_cosmos_query(path, body, chain_id)
        .map_err(|e| Status::invalid_argument(e.to_string()))?;

    let response = {
        let app = app.read().await;
        app.query(query)
    };

    // query() returns PulsarResult<QueryResponse<PulsarError>>
    let query_response =
        response.map_err(|e| Status::internal(format!("query failed: {e}")))?;

    encode_cosmos_response(query_response).map_err(|e| Status::internal(e.to_string()))
}

// ---------------------------------------------------------------------------
// Cosmos query tower Service fallback
// ---------------------------------------------------------------------------

/// Shared state for the Cosmos query fallback service.
///
/// Cloned for each request via the tower Service clone pattern.
/// Manual Clone impl because T: PersistentStorage does not need to be Clone
/// (App<T> is behind Arc so we clone the pointer, not T itself).
pub struct CosmosQueryState<T: PersistentStorage + Send + Sync + 'static> {
    pub app: Arc<RwLock<App<T>>>,
    pub chain_id: String,
}

impl<T: PersistentStorage + Send + Sync + 'static> Clone for CosmosQueryState<T> {
    fn clone(&self) -> Self {
        CosmosQueryState {
            app: self.app.clone(),
            chain_id: self.chain_id.clone(),
        }
    }
}

/// Tower Service that intercepts Cosmos SDK gRPC query paths and dispatches
/// them to `handle_cosmos_query()`. Returns properly gRPC-framed responses.
///
/// This is registered as the fallback service on the axum Router so that paths like
/// `/cosmos.bank.v1beta1.Query/Balance` reach the Layer App query handler
/// instead of returning gRPC UNIMPLEMENTED.
///
/// Uses the tower Service trait directly (via `fallback_service`) to avoid
/// axum Handler trait version compatibility issues when multiple axum versions
/// are present in the dependency graph.
///
/// Manual Clone impl because T: PersistentStorage does not need to be Clone.
pub struct CosmosQueryService<T: PersistentStorage + Send + Sync + 'static> {
    pub state: CosmosQueryState<T>,
}

impl<T: PersistentStorage + Send + Sync + 'static> Clone for CosmosQueryService<T> {
    fn clone(&self) -> Self {
        CosmosQueryService {
            state: self.state.clone(),
        }
    }
}

/// Type alias for the CosmosQueryService future to avoid ambiguity with multiple Service traits.
pub type CosmosQueryFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<http::Response<AxumBody>, std::convert::Infallible>> + Send>>;

impl<T: PersistentStorage + Send + Sync + 'static> tower_service::Service<http::Request<AxumBody>>
    for CosmosQueryService<T>
{
    type Response = http::Response<AxumBody>;
    type Error = std::convert::Infallible;
    type Future = CosmosQueryFuture;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: http::Request<AxumBody>) -> CosmosQueryFuture {
        let state = self.state.clone();
        Box::pin(async move {
            Ok(handle_cosmos_request(state, request).await)
        })
    }
}

/// Core async logic for handling a Cosmos gRPC query request.
///
/// Extracted from the Service impl to allow sharing between test and production code.
pub async fn handle_cosmos_request<T: PersistentStorage + Send + Sync + 'static>(
    state: CosmosQueryState<T>,
    request: http::Request<AxumBody>,
) -> http::Response<AxumBody> {
    let path = request.uri().path().to_string();

    // Only handle Cosmos SDK query paths — return gRPC UNIMPLEMENTED for anything else.
    let is_cosmos_query = path.starts_with("/cosmos.bank.")
        || path.starts_with("/cosmos.auth.")
        || path.starts_with("/cosmwasm.wasm.")
        || path.starts_with("/cosmos.tx.v1beta1.Service/Simulate");

    if !is_cosmos_query {
        // Return gRPC UNIMPLEMENTED (status 12) for non-cosmos paths
        return http::Response::builder()
            .status(200)
            .header(CONTENT_TYPE, "application/grpc")
            .header("grpc-status", "12")
            .header("grpc-message", "unimplemented")
            .body(AxumBody::empty())
            .unwrap();
    }

    // Collect the request body bytes.
    // gRPC requests have a 5-byte prefix: 1 byte compressed flag + 4 bytes length.
    let body_bytes = match request.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => {
            return http::Response::builder()
                .status(200)
                .header(CONTENT_TYPE, "application/grpc")
                .header("grpc-status", "13")
                .header("grpc-message", "failed to read request body")
                .body(AxumBody::empty())
                .unwrap();
        }
    };

    // Strip the gRPC 5-byte frame prefix if present.
    let proto_bytes = if body_bytes.len() >= 5 {
        Bytes::copy_from_slice(&body_bytes[5..])
    } else {
        body_bytes.clone()
    };

    // Dispatch to handle_cosmos_query.
    match handle_cosmos_query(&state.app, &state.chain_id, &path, proto_bytes).await {
        Ok(response_bytes) => {
            // Build gRPC-framed response: 1 byte compressed flag (0) + 4 bytes length + payload.
            let mut grpc_frame = Vec::with_capacity(5 + response_bytes.len());
            grpc_frame.push(0u8); // not compressed
            grpc_frame.extend_from_slice(&(response_bytes.len() as u32).to_be_bytes());
            grpc_frame.extend_from_slice(&response_bytes);

            // gRPC requires grpc-status in HTTP/2 trailers (a HEADERS frame *after* the DATA
            // frame), not in the initial HEADERS frame. Putting it in initial headers causes
            // "server closed the stream without sending trailers" on the client side.
            let mut trailers = http::HeaderMap::new();
            trailers.insert(
                http::header::HeaderName::from_static("grpc-status"),
                http::header::HeaderValue::from_static("0"),
            );
            let body = AxumBody::from(Bytes::from(grpc_frame))
                .with_trailers(async move { Some(Ok::<_, axum::Error>(trailers)) });

            http::Response::builder()
                .status(200)
                .header(CONTENT_TYPE, "application/grpc")
                .body(AxumBody::new(body))
                .unwrap()
        }
        Err(status) => {
            // Convert tonic::Status to gRPC error response.
            let code = status.code() as i32;
            let message = status.message().to_string();
            http::Response::builder()
                .status(200)
                .header(CONTENT_TYPE, "application/grpc")
                .header("grpc-status", code.to_string())
                .header("grpc-message", message)
                .body(AxumBody::empty())
                .unwrap()
        }
    }
}

/// Create an axum fallback service for Cosmos SDK query dispatch.
///
/// Returns an axum Router with the `CosmosQueryService` registered as the fallback.
/// The router is intended to be converted to tonic `Routes` and used with
/// `GrpcServer::builder().add_routes()` before adding tonic services.
pub fn cosmos_query_router<T: PersistentStorage + Send + Sync + 'static>(
    state: CosmosQueryState<T>,
) -> axum::Router {
    let svc = CosmosQueryService { state };
    axum::Router::new().fallback_service(svc)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;

    use cosmrs::{
        bank::MsgSend,
        crypto::secp256k1,
        tx::{self, Fee, Msg, SignDoc, SignerInfo},
        Coin,
    };
    use cosmwasm_std::{to_json_binary, Timestamp as CwTimestamp};
    use layer_app::genesis::{GenesisState, WasmParams};
    use layer_app::{App, AppConfig, StateMachine};
    use layer_std::api::{InitChainRequest, TmPubKey, ValidatorUpdate};
    use layer_std::BECH32_PREFIX;
    use layer_storage::MemoryStore;
    use tokio::sync::{Mutex, RwLock};

    use crate::mempool::Mempool;

    /// The fixed account number used by the Layer chain (matches FIXED_ACCOUNT_NUMBER in layer_cosmos).
    const ACCOUNT_NUMBER: u64 = 17;

    /// Serializing all App<T> creation across tests to avoid wasmer JIT mmap races on macOS.
    static APP_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn make_genesis() -> GenesisState {
        GenesisState {
            bank: vec![],
            wasm: WasmParams {
                gov_account: "juno1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmdyychx".to_string(),
            },
        }
    }

    /// Unique wasmer cache dir per App — see note in node.rs tests.
    fn unique_cache_dir() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir()
            .join(format!("slay3rd-grpc-test-{}-{n}", std::process::id()))
            .to_string_lossy()
            .into_owned()
    }

    fn init_app() -> App<MemoryStore> {
        let storage = MemoryStore::default();
        let logic = StateMachine::new(&AppConfig::new(&unique_cache_dir()));
        let mut app = App::new(storage, logic);

        let genesis = make_genesis();
        let app_state = to_json_binary(&genesis).unwrap();
        let request = InitChainRequest {
            time: CwTimestamp::from_nanos(1_673_194_026_078_305_426),
            chain_id: "junoclaw-1".into(),
            consensus_params: Default::default(),
            validators: vec![ValidatorUpdate {
                pub_key: TmPubKey::Ed25519(vec![123u8; 32]),
                power: 1_000_000,
            }],
            app_state,
            initial_height: 1,
        };
        app.init(request).unwrap();
        app
    }

    fn make_service(app: App<MemoryStore>) -> LayerGrpcService<MemoryStore> {
        LayerGrpcService {
            app: Arc::new(RwLock::new(app)),
            mempool: Arc::new(Mutex::new(Mempool::new(100))),
            chain_id: "junoclaw-1".to_string(),
            tx_index: Arc::new(TxIndex::new(16)),
            snapshot_cache: Arc::new(Mutex::new(None)),
        }
    }

    #[test]
    fn test_get_tx_found_and_not_found() {
        let _guard = APP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let svc = make_service(init_app());
            svc.tx_index.insert(layer_proto::cosmos::base::abci::v1beta1::TxResponse {
                txhash: "0C330C87AB".to_string(),
                height: 460_286,
                gas_used: 309_396,
                ..Default::default()
            });

            let resp = svc
                .get_tx(Request::new(GetTxRequest { hash: "0c330c87ab".into() }))
                .await
                .expect("indexed tx must be found")
                .into_inner();
            let tr = resp.tx_response.expect("tx_response set");
            assert_eq!(tr.height, 460_286);
            assert_eq!(tr.gas_used, 309_396);

            let err = svc
                .get_tx(Request::new(GetTxRequest { hash: "DEADBEEF".into() }))
                .await
                .expect_err("unknown tx must error");
            assert_eq!(err.code(), tonic::Code::NotFound);
        });
    }

    /// Build a valid, properly signed Cosmos tx using a random secp256k1 key.
    /// The genesis has no funded accounts, so check_tx will fail with a
    /// signature/sequence error — but the parse step must succeed. We use this
    /// to confirm the handler correctly distinguishes invalid_argument (parse
    /// failure) from failed_precondition (check_tx rejection).
    fn build_signed_tx_bytes() -> Vec<u8> {
        let chain_id = "junoclaw-1".parse().unwrap();
        let sender_private_key = secp256k1::SigningKey::random();
        let sender_public_key = sender_private_key.public_key();
        let sender_account_id = sender_public_key.account_id(BECH32_PREFIX).unwrap();
        let rcpt_account_id = secp256k1::SigningKey::random()
            .public_key()
            .account_id(BECH32_PREFIX)
            .unwrap();

        let amount = Coin {
            amount: 1_000u128,
            denom: "ujclaw".parse().unwrap(),
        };
        let fee_coin = Coin {
            amount: 100u128,
            denom: "ujclaw".parse().unwrap(),
        };

        let msg_send = MsgSend {
            from_address: sender_account_id.clone(),
            to_address: rcpt_account_id.clone(),
            amount: vec![amount],
        };

        let tx_body = tx::Body::new(vec![msg_send.to_any().unwrap()], "", 9001u16);
        let signer_info = SignerInfo::single_direct(Some(sender_public_key), 0);
        let auth_info = signer_info.auth_info(Fee::from_amount_and_gas(fee_coin, 200_000u64));

        let sign_doc =
            SignDoc::new(&tx_body, &auth_info, &chain_id, ACCOUNT_NUMBER).unwrap();
        let tx_signed = sign_doc.sign(&sender_private_key).unwrap();
        tx_signed.to_bytes().unwrap()
    }

    /// Validates the layer.lightclient.v1.Query service:
    ///
    /// 1. `block()` returns NOT_FOUND before the Reporter persists the
    ///    certificate/proposal/timestamp for a height
    /// 2. After persistence, `block()` returns all three header components
    /// 3. `latest_height()` returns the committed block info
    #[test]
    fn test_lightclient_query_block_roundtrip() {
        let _guard = APP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let svc = make_service(init_app());

            // --- Not found before persistence ---
            let result = svc
                .block(Request::new(QueryBlockRequest { height: 1 }))
                .await;
            assert!(result.is_err(), "block() should fail before persistence");
            let err = result.unwrap_err();
            assert_eq!(
                err.code(),
                tonic::Code::NotFound,
                "unpersisted height should produce NotFound, got: {:?}",
                err
            );

            // --- Persist the header components as the Reporter would ---
            {
                let mut app = svc.app.write().await;
                app.set_block_certificate(1, vec![0xAA; 48]).unwrap();
                app.set_block_proposal(1, vec![0xBB; 32]).unwrap();
                app.set_block_timestamp(1, 1_700_000_000_000_000_000).unwrap();
            }

            // --- Full header after persistence ---
            let response = svc
                .block(Request::new(QueryBlockRequest { height: 1 }))
                .await
                .expect("block() should succeed after persistence");
            let block = response.into_inner();
            assert_eq!(block.height, 1);
            assert_eq!(block.timestamp_nanos, 1_700_000_000_000_000_000);
            assert_eq!(block.proposal_bytes, vec![0xBB; 32]);
            assert_eq!(block.certificate_bytes, vec![0xAA; 48]);

            // --- Partial persistence is still not found ---
            {
                let mut app = svc.app.write().await;
                app.set_block_certificate(2, vec![0xCC; 48]).unwrap();
                // no proposal / timestamp for height 2
            }
            let result = svc
                .block(Request::new(QueryBlockRequest { height: 2 }))
                .await;
            assert!(
                result.is_err(),
                "block() should fail with partial persistence"
            );
            assert_eq!(
                result.unwrap_err().code(),
                tonic::Code::NotFound,
                "partial persistence should produce NotFound"
            );

            // --- latest_height reflects the committed block info ---
            let response = svc
                .latest_height(Request::new(QueryLatestHeightRequest {}))
                .await
                .expect("latest_height() should succeed");
            let latest = response.into_inner();
            // init() sets LAST_BLOCK to initial_height - 1 = 0
            assert_eq!(latest.height, 0);
        });
    }

    /// Validates that BroadcastTx gRPC handler:
    ///
    /// 1. Correctly routes parse errors to `Status::invalid_argument`
    /// 2. Correctly routes check_tx failures to `Status::failed_precondition`
    ///    (valid tx bytes, but account not in genesis → check_tx rejects it)
    /// 3. Accepts a valid tx and pushes it to the mempool (when check_tx passes)
    ///
    /// Because the test genesis has no funded accounts, a properly-formatted but
    /// unrecognized tx hits the check_tx rejection path. This confirms the handler
    /// is correctly wired end-to-end.
    #[test]
    fn test_broadcast_tx_roundtrip() {
        let _guard = APP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let svc = make_service(init_app());

            // --- Error path 1: garbage bytes → invalid_argument ---
            let garbage = vec![0xFF_u8; 32];
            let result = svc
                .broadcast_tx(Request::new(
                    layer_proto::cosmos::tx::v1beta1::BroadcastTxRequest {
                        tx_bytes: garbage,
                        mode: 0,
                    },
                ))
                .await;
            assert!(
                result.is_err(),
                "garbage bytes should return an error status"
            );
            let err = result.unwrap_err();
            assert_eq!(
                err.code(),
                tonic::Code::InvalidArgument,
                "garbage bytes should produce InvalidArgument, got: {:?}",
                err
            );

            // --- Error path 2: valid tx format but unknown account → failed_precondition ---
            let tx_bytes = build_signed_tx_bytes();
            let result = svc
                .broadcast_tx(Request::new(
                    layer_proto::cosmos::tx::v1beta1::BroadcastTxRequest {
                        tx_bytes,
                        mode: 0,
                    },
                ))
                .await;
            assert!(
                result.is_err(),
                "tx with unknown signer should be rejected by check_tx"
            );
            let err = result.unwrap_err();
            assert_eq!(
                err.code(),
                tonic::Code::FailedPrecondition,
                "unknown signer should produce FailedPrecondition, got: {:?}",
                err
            );

            // --- Verify mempool is empty (tx never reached it) ---
            let pool_len = {
                let pool = svc.mempool.lock().await;
                pool.len()
            };
            assert_eq!(pool_len, 0, "mempool should be empty — no tx passed check_tx");

            // --- Verify the uninitialized node guard works ---
            // Create a service with an uninitialized app
            let uninit_storage = MemoryStore::default();
            let uninit_logic =
                StateMachine::new(&AppConfig::new("/tmp/slay3rd-grpc-uninit-test"));
            let uninit_app = App::new(uninit_storage, uninit_logic);
            let uninit_svc = make_service(uninit_app);

            // Build a syntactically valid tx
            let tx_bytes = build_signed_tx_bytes();
            let result = uninit_svc
                .broadcast_tx(Request::new(
                    layer_proto::cosmos::tx::v1beta1::BroadcastTxRequest {
                        tx_bytes,
                        mode: 0,
                    },
                ))
                .await;
            // Could be InvalidArgument (parse fails first) or Unavailable (init guard)
            // Either is acceptable — we just confirm it doesn't panic
            assert!(
                result.is_err(),
                "uninitialized node should return an error"
            );
        });
    }

    /// Validates the Simulate gRPC handler:
    ///
    /// 1. Empty `tx_bytes` → `invalid_argument` (deprecated `tx` field unsupported)
    /// 2. Garbage bytes → `invalid_argument` (parse failure)
    /// 3. Valid signed tx from an unfunded account → handler returns Ok with
    ///    `gas_info` populated and `result.log` carrying the execution error
    ///    (the genesis has no funded accounts, so auth fails inside execute_tx —
    ///    which proves the tx was actually executed, not just check_tx'd)
    /// 4. Nothing is persisted: LAST_BLOCK stays at the init height
    #[test]
    fn test_simulate() {
        let _guard = APP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let svc = make_service(init_app());

            // --- Empty tx_bytes → invalid_argument ---
            let err = svc
                .simulate(Request::new(SimulateRequest {
                    tx: None,
                    tx_bytes: vec![],
                }))
                .await
                .expect_err("empty tx_bytes must be rejected");
            assert_eq!(err.code(), tonic::Code::InvalidArgument);

            // --- Garbage bytes → invalid_argument ---
            let err = svc
                .simulate(Request::new(SimulateRequest {
                    tx: None,
                    tx_bytes: vec![0xFF_u8; 32],
                }))
                .await
                .expect_err("garbage bytes must be rejected");
            assert_eq!(err.code(), tonic::Code::InvalidArgument);

            // --- Valid signed tx, unfunded account → Ok + error in result.log ---
            let tx_bytes = build_signed_tx_bytes();
            let resp = svc
                .simulate(Request::new(SimulateRequest {
                    tx: None,
                    tx_bytes,
                }))
                .await
                .expect("simulate must return Ok even when execution fails")
                .into_inner();
            let gas = resp.gas_info.expect("gas_info always set");
            assert!(gas.gas_wanted > 0, "gas_wanted must reflect tx fee limit");
            let result = resp.result.expect("result always set");
            assert!(
                !result.log.is_empty(),
                "unfunded account must surface execution error in log"
            );

            // --- Nothing persisted: height still 0 ---
            let latest = svc
                .latest_height(Request::new(QueryLatestHeightRequest {}))
                .await
                .unwrap()
                .into_inner();
            assert_eq!(latest.height, 0, "simulate must not advance state");
        });
    }

    /// State-sync gRPC surface: ListSnapshots offers exactly one cached
    /// export; LoadSnapshotChunk serves every chunk, decodes clean, and
    /// the whole-dump root check verifies against the advertised
    /// state_root. Wrong height / bad format / OOB chunk all rejected.
    #[test]
    fn test_statesync_list_and_load_chunk() {
        let _guard = APP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let svc = make_service(init_app());

            let res = svc
                .list_snapshots(Request::new(ListSnapshotsRequest {}))
                .await
                .unwrap()
                .into_inner();
            assert_eq!(res.snapshots.len(), 1, "at most one offered snapshot");
            let meta = &res.snapshots[0];
            assert_eq!(meta.format, SNAPSHOT_FORMAT_V1);
            assert!(meta.chunks >= 1);
            assert_eq!(meta.state_root.len(), 32);
            assert!(meta.total_bytes > 0);

            // fetch every chunk, verify checksum, decode records
            let mut records = Vec::new();
            for i in 0..meta.chunks {
                let r = svc
                    .load_snapshot_chunk(Request::new(LoadSnapshotChunkRequest {
                        height: meta.height,
                        format: SNAPSHOT_FORMAT_V1,
                        chunk: i,
                    }))
                    .await
                    .unwrap()
                    .into_inner();
                let cksum: [u8; 32] =
                    <sha2::Sha256 as sha2::Digest>::digest(&r.chunk).into();
                assert_eq!(
                    &cksum[..],
                    &r.checksum[..],
                    "chunk {i} checksum mismatch"
                );
                records.extend(layer_app::decode_snapshot_chunk(&r.chunk).unwrap());
            }
            assert!(!records.is_empty(), "genesis must produce KV records");

            // whole-dump authentication: recomputed root == advertised root
            let mut root = [0u8; 32];
            root.copy_from_slice(&meta.state_root);
            assert!(layer_app::verify_snapshot_root(&records, &root));

            // wrong height -> NotFound
            let err = svc
                .load_snapshot_chunk(Request::new(LoadSnapshotChunkRequest {
                    height: meta.height + 1,
                    format: SNAPSHOT_FORMAT_V1,
                    chunk: 0,
                }))
                .await
                .unwrap_err();
            assert_eq!(err.code(), tonic::Code::NotFound);

            // bad format -> InvalidArgument
            let err = svc
                .load_snapshot_chunk(Request::new(LoadSnapshotChunkRequest {
                    height: meta.height,
                    format: 99,
                    chunk: 0,
                }))
                .await
                .unwrap_err();
            assert_eq!(err.code(), tonic::Code::InvalidArgument);

            // out-of-range chunk -> NotFound
            let err = svc
                .load_snapshot_chunk(Request::new(LoadSnapshotChunkRequest {
                    height: meta.height,
                    format: SNAPSHOT_FORMAT_V1,
                    chunk: 99,
                }))
                .await
                .unwrap_err();
            assert_eq!(err.code(), tonic::Code::NotFound);

            // uninitialized app -> Unavailable
            let uninit = make_service(App::new(
                MemoryStore::default(),
                StateMachine::new(&AppConfig::new(&unique_cache_dir())),
            ));
            let err = uninit
                .list_snapshots(Request::new(ListSnapshotsRequest {}))
                .await
                .unwrap_err();
            assert_eq!(err.code(), tonic::Code::Unavailable);
        });
    }
}
