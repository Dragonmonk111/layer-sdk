use std::sync::Arc;

use pulsar_proto::cosmwasm::wasm::v1::{
    query_server::{Query, QueryServer},
    QueryAllContractStateRequest, QueryAllContractStateResponse, QueryCodeRequest,
    QueryCodeResponse, QueryCodesRequest, QueryCodesResponse, QueryContractHistoryRequest,
    QueryContractHistoryResponse, QueryContractInfoRequest, QueryContractInfoResponse,
    QueryContractsByCodeRequest, QueryContractsByCodeResponse, QueryParamsRequest,
    QueryParamsResponse, QueryPinnedCodesRequest, QueryPinnedCodesResponse,
    QueryRawContractStateRequest, QueryRawContractStateResponse, QuerySmartContractStateRequest,
    QuerySmartContractStateResponse,
};

use pulsar_abci::MultiThreadedDispatcher;
use tonic::{Request, Response, Status};

use super::{abci_response_to_grpc, grpc_request_to_abci};

pub fn cosmwasm_service(dispatcher: Arc<MultiThreadedDispatcher>) -> QueryServer<CosmWasmService> {
    QueryServer::new(CosmWasmService::new(dispatcher))
}

pub struct CosmWasmService {
    dispatcher: Arc<MultiThreadedDispatcher>,
}

impl CosmWasmService {
    pub fn new(dispatcher: Arc<MultiThreadedDispatcher>) -> Self {
        Self { dispatcher }
    }
}

#[tonic::async_trait]
impl Query for CosmWasmService {
    /// ContractInfo gets the contract meta data
    async fn contract_info(
        &self,
        request: Request<QueryContractInfoRequest>,
    ) -> Result<Response<QueryContractInfoResponse>, Status> {
        let query = grpc_request_to_abci("/cosmwasm.wasm.v1.Query/ContractInfo", request.get_ref());
        let response = self.dispatcher.dispatch_query(query).await;
        abci_response_to_grpc(response).map(Response::new)
    }

    /// ContractHistory gets the contract code history
    async fn contract_history(
        &self,
        _request: Request<QueryContractHistoryRequest>,
    ) -> Result<Response<QueryContractHistoryResponse>, Status> {
        unimplemented!();
    }
    /// ContractsByCode lists all smart contracts for a code id
    async fn contracts_by_code(
        &self,
        _request: Request<QueryContractsByCodeRequest>,
    ) -> Result<Response<QueryContractsByCodeResponse>, Status> {
        unimplemented!();
    }
    /// AllContractState gets all raw store data for a single contract
    async fn all_contract_state(
        &self,
        _request: Request<QueryAllContractStateRequest>,
    ) -> Result<Response<QueryAllContractStateResponse>, Status> {
        unimplemented!();
    }
    /// RawContractState gets single key from the raw store data of a contract
    async fn raw_contract_state(
        &self,
        request: Request<QueryRawContractStateRequest>,
    ) -> Result<Response<QueryRawContractStateResponse>, Status> {
        let query = grpc_request_to_abci(
            "/cosmwasm.wasm.v1.Query/RawContractState",
            request.get_ref(),
        );
        let response = self.dispatcher.dispatch_query(query).await;
        abci_response_to_grpc(response).map(Response::new)
    }

    /// SmartContractState get smart query result from the contract
    async fn smart_contract_state(
        &self,
        request: Request<QuerySmartContractStateRequest>,
    ) -> Result<Response<QuerySmartContractStateResponse>, Status> {
        let query = grpc_request_to_abci(
            "/cosmwasm.wasm.v1.Query/SmartContractState",
            request.get_ref(),
        );
        let response = self.dispatcher.dispatch_query(query).await;
        abci_response_to_grpc(response).map(Response::new)
    }
    /// Code gets the binary code and metadata for a singe wasm code
    async fn code(
        &self,
        request: Request<QueryCodeRequest>,
    ) -> Result<Response<QueryCodeResponse>, Status> {
        let query = grpc_request_to_abci("/cosmwasm.wasm.v1.Query/Code", request.get_ref());
        let response = self.dispatcher.dispatch_query(query).await;
        abci_response_to_grpc(response).map(Response::new)
    }

    /// Codes gets the metadata for all stored wasm codes
    async fn codes(
        &self,
        _request: Request<QueryCodesRequest>,
    ) -> Result<Response<QueryCodesResponse>, Status> {
        unimplemented!();
    }

    /// PinnedCodes gets the pinned code ids
    async fn pinned_codes(
        &self,
        _request: Request<QueryPinnedCodesRequest>,
    ) -> Result<Response<QueryPinnedCodesResponse>, Status> {
        unimplemented!();
    }

    /// Params gets the module params
    async fn params(
        &self,
        _request: Request<QueryParamsRequest>,
    ) -> Result<Response<QueryParamsResponse>, Status> {
        unimplemented!();
    }
}
