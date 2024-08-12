use std::sync::Arc;

use slay3r_abci::MultiThreadedDispatcher;
use slay3r_proto::layer::sync::v1::{
    query_server::{Query, QueryServer},
    BlockWrites, QueryLatestHeightRequest, QueryLatestHeightResponse, StreamChangesSinceRequest,
    StreamCurrentStateRequest, WriteData,
};
// use ibc_proto::cosmos::base::v1beta1::Coin as RawCoin;
use tonic::{Request, Response, Status};

// use super::{abci_response_to_grpc, grpc_request_to_abci, unimplemented};

pub fn sync_service(dispatcher: Arc<MultiThreadedDispatcher>) -> QueryServer<SyncService> {
    QueryServer::new(SyncService::new(dispatcher))
}

pub struct SyncService {
    _dispatcher: Arc<MultiThreadedDispatcher>,
}

impl SyncService {
    pub fn new(dispatcher: Arc<MultiThreadedDispatcher>) -> Self {
        Self {
            _dispatcher: dispatcher,
        }
    }
}

// TODO: implement
pub struct CurrentStateStream;

impl tonic::codegen::tokio_stream::Stream for CurrentStateStream {
    type Item = Result<WriteData, Status>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::task::Poll::Ready(None)
    }
}

// TODO: implement
pub struct ChangesSinceStream;

impl tonic::codegen::tokio_stream::Stream for ChangesSinceStream {
    type Item = Result<BlockWrites, Status>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::task::Poll::Ready(None)
    }
}

#[tonic::async_trait]
impl Query for SyncService {
    type ChangesSinceStream = ChangesSinceStream;
    type CurrentStateStream = CurrentStateStream;

    #[tracing::instrument(skip(self), level = "info")]
    async fn latestheight(
        &self,
        _request: Request<QueryLatestHeightRequest>,
    ) -> Result<Response<QueryLatestHeightResponse>, Status> {
        // TODO: implement
        Ok(Response::new(QueryLatestHeightResponse { height: 17 }))
    }

    #[tracing::instrument(skip(self), level = "info")]
    async fn current_state(
        &self,
        _request: Request<StreamCurrentStateRequest>,
    ) -> Result<Response<Self::CurrentStateStream>, Status> {
        // TODO: implement
        Ok(Response::new(CurrentStateStream))
    }

    /// TODO: return block by block?
    /// Then it is one sequence number with a block of write/read requests
    /// Which will map much nicer to the rocksdb implementation
    #[tracing::instrument(skip(self), level = "info")]
    async fn changes_since(
        &self,
        _request: Request<StreamChangesSinceRequest>,
    ) -> Result<Response<Self::ChangesSinceStream>, Status> {
        // TODO: implement
        Ok(Response::new(ChangesSinceStream))
    }
}
