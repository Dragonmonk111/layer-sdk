use pulsar_std::response::{BankQueryResponse, QueryResponse};
use pulsar_std::{AccountId, BankQuery, Query, QueryError};

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

// FIXME: what is this used for? defined in
pub const QUERY_PATH_CUSTOM: &str = "custom";

/// Handled by tendermint p2p layer, ignore it
pub const QUERY_PATH_P2P: &str = "p2p";

/// /store/{substore}/{key} returns ??
/// Check out CMS queryable interface
/// https://github.com/cosmos/cosmos-sdk/blob/v0.47.2/baseapp/abci.go#L920-L941
pub const QUERY_PATH_STORE: &str = "store";

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
        QUERY_PATH_P2P => Err(QueryError::UnsupportedPath(QUERY_PATH_P2P.to_string())),
        QUERY_PATH_CUSTOM => Err(QueryError::UnsupportedPath(QUERY_PATH_CUSTOM.to_string())),
        p => Err(QueryError::UnsupportedPath(p.to_string())),
    }
}

/// This will use grpc path lookups, returns Ok(None) if not a match, so we try special queries
pub fn parse_cosmos_grpc_query(path: &str, data: &[u8]) -> Result<Option<Query>, QueryError> {
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
        _ => todo!(),
    }
}

pub fn encode_cosmos_response(res: &QueryResponse) -> Vec<u8> {
    match res {
        QueryResponse::Raw { value } => value.clone(),
        QueryResponse::Bank(bank) => encode_bank_response(bank),
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
