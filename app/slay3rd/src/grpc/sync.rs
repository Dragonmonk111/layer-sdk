use std::ops::Deref;
use tonic::{Request, Response, Status};

use slay3r_app::SyncProvider;
use slay3r_proto::layer::sync::v1::{
    query_server::{Query, QueryServer},
    BlockWrites, QueryLatestHeightRequest, QueryLatestHeightResponse, StreamChangesSinceRequest,
    StreamCurrentStateRequest, WriteData,
};
use slay3r_storage::PersistentStorage;

use crate::app::Pulsarium;

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

pub struct SyncStream<T>(Box<dyn Iterator<Item = Result<T, String>> + Send>);

use std::ops::DerefMut;

impl<T> tonic::codegen::tokio_stream::Stream for SyncStream<T> {
    type Item = Result<T, Status>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let item = self.deref_mut().0.next();
        // change the error type from string to tonic::Status
        let out = item.map(|x| x.map_err(Status::internal));
        std::task::Poll::Ready(out)
    }
}

#[tonic::async_trait]
impl<T: PersistentStorage + 'static + Send + Sync> Query for SyncService<T> {
    type ChangesSinceStream = SyncStream<BlockWrites>;
    type CurrentStateStream = SyncStream<WriteData>;

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
        let stream = lock.deref().current_state();
        Ok(Response::new(SyncStream(stream)))
    }

    #[tracing::instrument(skip(self), level = "info")]
    async fn changes_since(
        &self,
        request: Request<StreamChangesSinceRequest>,
    ) -> Result<Response<Self::ChangesSinceStream>, Status> {
        let lock = self.app.sync();
        let stream = lock.deref().changes_since(request.get_ref().height);
        Ok(Response::new(SyncStream(stream)))
    }
}
