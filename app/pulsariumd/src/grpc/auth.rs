use ibc_proto::cosmos::auth::v1beta1::{
    query_server::{Query, QueryServer},
    AddressBytesToStringRequest, AddressBytesToStringResponse, AddressStringToBytesRequest,
    AddressStringToBytesResponse, Bech32PrefixRequest, Bech32PrefixResponse,
    QueryAccountAddressByIdRequest, QueryAccountAddressByIdResponse, QueryAccountRequest,
    QueryAccountResponse, QueryAccountsRequest, QueryAccountsResponse,
    QueryModuleAccountByNameRequest, QueryModuleAccountByNameResponse, QueryModuleAccountsRequest,
    QueryModuleAccountsResponse, QueryParamsRequest, QueryParamsResponse,
};

use tonic::{Request, Response, Status};

pub fn auth_service() -> QueryServer<AuthService> {
    QueryServer::new(AuthService::new())
}

pub struct AuthService {}

impl AuthService {
    pub fn new() -> Self {
        Self {}
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
        _request: Request<QueryAccountRequest>,
    ) -> Result<Response<QueryAccountResponse>, Status> {
        unimplemented!()
    }

    async fn params(
        &self,
        _request: Request<QueryParamsRequest>,
    ) -> Result<Response<QueryParamsResponse>, Status> {
        unimplemented!()
    }

    async fn account_address_by_id(
        &self,
        _request: Request<QueryAccountAddressByIdRequest>,
    ) -> Result<Response<QueryAccountAddressByIdResponse>, Status> {
        unimplemented!()
    }

    async fn module_accounts(
        &self,
        _request: Request<QueryModuleAccountsRequest>,
    ) -> Result<Response<QueryModuleAccountsResponse>, Status> {
        unimplemented!()
    }

    async fn module_account_by_name(
        &self,
        _request: Request<QueryModuleAccountByNameRequest>,
    ) -> Result<Response<QueryModuleAccountByNameResponse>, Status> {
        unimplemented!()
    }

    async fn bech32_prefix(
        &self,
        _request: Request<Bech32PrefixRequest>,
    ) -> Result<Response<Bech32PrefixResponse>, Status> {
        unimplemented!()
    }

    async fn address_bytes_to_string(
        &self,
        _request: Request<AddressBytesToStringRequest>,
    ) -> Result<Response<AddressBytesToStringResponse>, Status> {
        unimplemented!()
    }

    async fn address_string_to_bytes(
        &self,
        _request: Request<AddressStringToBytesRequest>,
    ) -> Result<Response<AddressStringToBytesResponse>, Status> {
        unimplemented!()
    }
}
