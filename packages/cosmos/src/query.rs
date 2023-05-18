use pulsar_std::response::{AuthQueryResponse, BankQueryResponse, QueryResponse};
use pulsar_std::{AccountId, AuthQuery, BankQuery, Query, QueryError};

use cosmos_sdk_proto::cosmos::auth::v1beta1::QueryAccountRequest;
use cosmos_sdk_proto::cosmos::bank::v1beta1::{
    QueryAllBalancesRequest, QueryAllBalancesResponse, QueryBalanceRequest, QueryBalanceResponse,
    QuerySupplyOfRequest, QuerySupplyOfResponse,
};
use cosmos_sdk_proto::prost::Message;

use crate::utils::{encode_sdk_coin, encode_sdk_coins};
use crate::CosmosError;

// "/app" prefix for special application queries
// /app/simulate returns JSON of {GasInfo, Result}
// /app/version returns app version string cast to bytes
pub const QUERY_PATH_APP: &str = "app";

/// /store/{substore}/{key} returns ??
/// Check out CMS queryable interface
/// https://github.com/cosmos/cosmos-sdk/blob/v0.47.2/baseapp/abci.go#L920-L941
pub const QUERY_PATH_STORE: &str = "store";

// These two exist in Cosmos SDK, but we don't use them
// const QUERY_PATH_CUSTOM: &str = "custom";
// const QUERY_PATH_P2P: &str = "p2p";

// See relevant code we emulate at https://github.com/cosmos/cosmos-sdk/blob/v0.47.2/baseapp/abci.go#L538-L561
pub fn parse_cosmos_query(path: &str, _data: &[u8]) -> Result<Query, QueryError> {
    if let Some(grpc_res) = parse_cosmos_grpc_query(path, _data)? {
        return Ok(grpc_res);
    }

    // try some special cases
    let fragments: Vec<&str> = path.split('/').collect();
    match fragments[0] {
        QUERY_PATH_APP => todo!(),   // add simulate support
        QUERY_PATH_STORE => todo!(), // for raw queries
        p => Err(QueryError::UnsupportedPath(p.to_string())),
    }
}

/// This will use grpc path lookups, returns Ok(None) if not a match, so we try special queries
pub fn parse_cosmos_grpc_query(path: &str, data: &[u8]) -> Result<Option<Query>, QueryError> {
    // TODO: add auth queries
    // FIXME: make more extensible when we add cosmwasm, etc support
    match path {
        // see https://github.com/cosmos/cosmos-rust/blob/main/cosmos-sdk-proto/src/prost/cosmos-sdk/cosmos.bank.v1beta1.tonic.rs#L85
        "/cosmos.bank.v1beta1.Query/Balance" => {
            let req = QueryBalanceRequest::decode(data).map_err(CosmosError::from)?;
            let address = AccountId::parse_string(&req.address)?;
            let denom = req.denom;
            let query = BankQuery::Balance { address, denom };
            Ok(Some(query.into()))
        }
        "/cosmos.bank.v1beta1.Query/AllBalances" => {
            let req = QueryAllBalancesRequest::decode(data).map_err(CosmosError::from)?;
            let address = AccountId::parse_string(&req.address)?;
            let query = BankQuery::AllBalances { address };
            Ok(Some(query.into()))
        }
        "/cosmos.bank.v1beta1.Query/SupplyOf" => {
            let req = QuerySupplyOfRequest::decode(data).map_err(CosmosError::from)?;
            let denom = req.denom;
            let query = BankQuery::Supply { denom };
            Ok(Some(query.into()))
        }
        "/cosmos.auth.v1beta1.Query/Account" => {
            let req = QueryAccountRequest::decode(data).map_err(CosmosError::from)?;
            let address = AccountId::parse_string(&req.address)?;
            let query = AuthQuery::Account { address };
            Ok(Some(query.into()))
        }
        // "/cosmos.auth.v1beta1.Query/Accounts" => {
        //     let _ = QueryAccountsRequest::decode(data).map_err(CosmosError::from)?;
        //     todo!();
        // }
        // "/cosmos.auth.v1beta1.Query/Params" => {
        //     let _ = QueryParamsRequest::decode(data).map_err(CosmosError::from)?;
        //     todo!();
        // }
        _ => Ok(None),
    }
}

pub fn encode_cosmos_response(res: &QueryResponse) -> Vec<u8> {
    match res {
        QueryResponse::Raw { value } => value.clone(),
        QueryResponse::Auth(auth) => encode_auth_response(auth),
        QueryResponse::Bank(bank) => encode_bank_response(bank),
    }
}

pub fn encode_auth_response(res: &AuthQueryResponse) -> Vec<u8> {
    match res {
        AuthQueryResponse::Account(_) => {
            // QueryAccountResponse { account: Some(r.account.clone()) }.encode_to_vec()
            todo!();
        }
    }
}

pub fn encode_bank_response(res: &BankQueryResponse) -> Vec<u8> {
    match res {
        BankQueryResponse::Balance(r) => {
            let balance = Some(encode_sdk_coin(&r.amount));
            QueryBalanceResponse { balance }.encode_to_vec()
        }
        BankQueryResponse::AllBalances(r) => QueryAllBalancesResponse {
            balances: encode_sdk_coins(&r.amount),
            pagination: None,
        }
        .encode_to_vec(),
        BankQueryResponse::Supply(r) => {
            let amount = Some(encode_sdk_coin(&r.amount));
            QuerySupplyOfResponse { amount }.encode_to_vec()
        }
    }
}
