// use std::sync::Arc;
use std::ops::Deref;

use slay3r_app::SyncProvider;
use slay3r_proto::layer::sync::v1::{
    query_server::{Query, QueryServer},
    BlockWrites, QueryLatestHeightRequest, QueryLatestHeightResponse, StreamChangesSinceRequest,
    StreamCurrentStateRequest, WriteData,
};
use slay3r_storage::PersistentStorage;
// use ibc_proto::cosmos::base::v1beta1::Coin as RawCoin;
use tonic::{Request, Response, Status};

use crate::app::Pulsarium;

// use super::{abci_response_to_grpc, grpc_request_to_abci, unimplemented};

pub fn sync_service<T: PersistentStorage + 'static + Send + Sync>(
    app: Pulsarium<T>,
) -> QueryServer<SyncService<T>> {
    QueryServer::new(SyncService::new(app))
}

pub struct SyncService<T: PersistentStorage + 'static + Send + Sync> {
    app: Pulsarium<T>,
}

impl<T: PersistentStorage + 'static + Send + Sync> SyncService<T> {
    pub fn new(app: Pulsarium<T>) -> Self {
        Self { app }
    }
}

pub struct ChangesSinceStream(Box<dyn Iterator<Item = BlockWrites> + Send>);

use std::ops::DerefMut;

impl tonic::codegen::tokio_stream::Stream for ChangesSinceStream {
    type Item = Result<BlockWrites, Status>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let item = self.deref_mut().0.next();
        std::task::Poll::Ready(item.map(Ok))
    }
}

#[tonic::async_trait]
impl<T: PersistentStorage + 'static + Send + Sync> Query for SyncService<T> {
    type ChangesSinceStream = ChangesSinceStream;
    type CurrentStateStream = tokio_stream::Iter<std::vec::IntoIter<Result<WriteData, Status>>>;

    #[tracing::instrument(skip(self), level = "info")]
    async fn latestheight(
        &self,
        _request: Request<QueryLatestHeightRequest>,
    ) -> Result<Response<QueryLatestHeightResponse>, Status> {
        let lock = self.app.sync();
        let seq = lock.deref().latest_sequence();
        Ok(Response::new(QueryLatestHeightResponse { height: seq }))
    }

    #[tracing::instrument(skip(self), level = "info")]
    async fn current_state(
        &self,
        _request: Request<StreamCurrentStateRequest>,
    ) -> Result<Response<Self::CurrentStateStream>, Status> {
        let lock = self.app.sync();
        // TODO: don't read all into memory, but we cannot hold a ref to the DB
        // Most obvious solution is paginaton
        let state: Vec<Result<_, Status>> = lock.deref().current_state().map(Ok).collect();
        let stream = tokio_stream::iter(state);
        Ok(Response::new(stream))
    }

    #[tracing::instrument(skip(self), level = "info")]
    async fn changes_since(
        &self,
        request: Request<StreamChangesSinceRequest>,
    ) -> Result<Response<Self::ChangesSinceStream>, Status> {
        let lock = self.app.sync();
        let stream = lock.deref().changes_since(request.get_ref().height);
        Ok(Response::new(ChangesSinceStream(stream)))
    }
}
