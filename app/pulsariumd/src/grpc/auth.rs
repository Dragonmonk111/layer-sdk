use std::sync::Arc;

use pulsar_proto::cosmos::auth::v1beta1::{
    query_server::{Query, QueryServer},
    QueryAccountRequest, QueryAccountResponse, QueryAccountsRequest, QueryAccountsResponse,
    QueryModuleAccountByNameRequest, QueryModuleAccountByNameResponse, QueryParamsRequest,
    QueryParamsResponse,
};

use pulsar_abci::MultiThreadedDispatcher;
use tonic::{Request, Response, Status};

use super::{abci_response_to_grpc, grpc_request_to_abci};

pub fn auth_service(dispatcher: Arc<MultiThreadedDispatcher>) -> QueryServer<AuthService> {
    QueryServer::new(AuthService::new(dispatcher))
}

pub struct AuthService {
    dispatcher: Arc<MultiThreadedDispatcher>,
}

impl AuthService {
    pub fn new(dispatcher: Arc<MultiThreadedDispatcher>) -> Self {
        Self { dispatcher }
    }
}

#[tonic::async_trait]
impl Query for AuthService {
    async fn accounts(
        &self,
        _request: Request<QueryAccountsRequest>,
    ) -> Result<Response<QueryAccountsResponse>, Status> {
        unimplemented!()
    }

    async fn account(
        &self,
        request: Request<QueryAccountRequest>,
    ) -> Result<Response<QueryAccountResponse>, Status> {
        let query = grpc_request_to_abci("/cosmos.auth.v1beta1.Query/Account", request.get_ref());
        let response = self.dispatcher.dispatch_query(query).await;
        abci_response_to_grpc::<QueryAccountResponse>(response).map(Response::new)
    }

    async fn params(
        &self,
        _request: Request<QueryParamsRequest>,
    ) -> Result<Response<QueryParamsResponse>, Status> {
        unimplemented!()
    }

    async fn module_account_by_name(
        &self,
        _request: Request<QueryModuleAccountByNameRequest>,
    ) -> Result<Response<QueryModuleAccountByNameResponse>, Status> {
        unimplemented!()
    }
}
