use cosmos_sdk_proto::cosmos::tx::v1beta1::SimulateResponse;
use cosmwasm_std::Event;
use pulsar_std::api::{GasInfo, TxResponse, TxResult};
use pulsar_std::response::{AccountResponse, AuthQueryResponse, BankQueryResponse, QueryResponse};
use pulsar_std::{AccountId, AuthQuery, BankQuery, Query, QueryError};

use cosmos_sdk_proto::cosmos::auth::v1beta1::{
    BaseAccount, QueryAccountRequest, QueryAccountResponse,
};
use cosmos_sdk_proto::cosmos::bank::v1beta1::{
    QueryAllBalancesRequest, QueryAllBalancesResponse, QueryBalanceRequest, QueryBalanceResponse,
    QuerySupplyOfRequest, QuerySupplyOfResponse,
};
use cosmos_sdk_proto::prost::Message;
use cosmos_sdk_proto::traits::{MessageExt, TypeUrl};

use crate::pubkey::encode_cosmos_pubkey;
use crate::tx::FIXED_ACCOUNT_NUMBER;
use crate::utils::{encode_sdk_coin, encode_sdk_coins};
use crate::{parse_cosmos_tx, CosmosError};

/// "/app" prefix for special application queries
/// /app/version returns app version string cast to bytes
pub const QUERY_PATH_APP: &str = "app";

/// /store/{substore}/{key} returns ??
/// Check out CMS queryable interface
/// https://github.com/cosmos/cosmos-sdk/blob/v0.47.2/baseapp/abci.go#L920-L941
pub const QUERY_PATH_STORE: &str = "store";

// These two exist in Cosmos SDK, but we don't use them
// const QUERY_PATH_CUSTOM: &str = "custom";
// const QUERY_PATH_P2P: &str = "p2p";

// See relevant code we emulate at https://github.com/cosmos/cosmos-sdk/blob/v0.47.2/baseapp/abci.go#L538-L561
pub fn parse_cosmos_query(path: &str, data: &[u8], chain_id: &str) -> Result<Query, QueryError> {
    if let Some(grpc_res) = parse_cosmos_grpc_query(path, data)? {
        return Ok(grpc_res);
    }

    // try some special cases
    let fragments: Vec<&str> = path.split('/').collect();
    match fragments[0] {
        QUERY_PATH_APP => {
            if fragments.len() != 2 {
                return Err(QueryError::UnsupportedPath(path.to_string()));
            }
            parse_app_query(fragments[1], data, chain_id)
        }
        QUERY_PATH_STORE => parse_store_query(&fragments[1..], data),
        p => Err(QueryError::UnsupportedPath(p.to_string())),
    }
}

/// for raw queries
fn parse_store_query(_fragments: &[&str], data: &[u8]) -> Result<Query, QueryError> {
    // FIXME: review if this is correct when we have a sample caller for compatibility
    Ok(Query::Raw { key: data.to_vec() })
}

/// simulate and version support
fn parse_app_query(command: &str, data: &[u8], chain_id: &str) -> Result<Query, QueryError> {
    match command {
        "simulate" => {
            // FIXME: error handling is ugly, revise proper types
            let tx = parse_cosmos_tx(data, chain_id)
                .map_err(|e| QueryError::ParseError(e.to_string()))?;
            Ok(Query::Simulate(tx))
        }
        "version" => todo!(),
        _ => Err(QueryError::UnsupportedPath(format!(
            "{}/{}",
            QUERY_PATH_APP, command
        ))),
    }
}

/// This will use grpc path lookups, returns Ok(None) if not a match, so we try special queries
fn parse_cosmos_grpc_query(path: &str, data: &[u8]) -> Result<Option<Query>, QueryError> {
    // FIXME: add auth queries
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
        //     unimplemented!();
        // }
        // "/cosmos.auth.v1beta1.Query/Params" => {
        //     let _ = QueryParamsRequest::decode(data).map_err(CosmosError::from)?;
        //     unimplemented!();
        // }
        _ => Ok(None),
    }
}

pub fn encode_cosmos_response<E: std::error::Error>(
    res: &QueryResponse<E>,
) -> Result<Vec<u8>, QueryError> {
    match res {
        QueryResponse::Raw { key: _, value } => Ok(value.clone()),
        QueryResponse::Auth(auth) => encode_auth_response(auth),
        QueryResponse::Bank(bank) => Ok(encode_bank_response(bank)),
        QueryResponse::Simulate(simulate) => encode_simulate_response(simulate),
    }
}

pub fn encode_auth_response(res: &AuthQueryResponse) -> Result<Vec<u8>, QueryError> {
    match res {
        AuthQueryResponse::Account(acc) => {
            let (address, pub_key, sequence) = match acc {
                AccountResponse::External {
                    address,
                    pubkey,
                    sequence,
                } => {
                    let pub_key = pubkey.as_ref().map(encode_cosmos_pubkey).transpose()?;
                    (address.to_string(), pub_key, *sequence)
                }
                AccountResponse::Internal { address } => (address.to_string(), None, 0),
                AccountResponse::Smart { address, .. } => (address.to_string(), None, 0),
            };
            let base = BaseAccount {
                address,
                account_number: FIXED_ACCOUNT_NUMBER,
                sequence,
                pub_key,
            };
            let account = Some(
                base.to_any()
                    .map_err(|_| QueryError::EncodingError("to_any".to_string()))?,
            );
            Ok(QueryAccountResponse { account }.encode_to_vec())
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

pub fn encode_simulate_response<E: std::error::Error>(
    result: &TxResult<E>,
) -> Result<Vec<u8>, QueryError> {
    let gas_info = encode_gas_info(&result.gas);
    // if the result was an error, we just return error to the query service, it gets encoded at rpc level
    let r = result
        .result
        .as_ref()
        .map_err(|e| QueryError::EncodingError(e.to_string()))?;
    let result = encode_tx_result(r);

    let sim = SimulateResponse {
        gas_info: Some(gas_info),
        result: Some(result),
    };
    Ok(sim.encode_to_vec())
}

fn encode_gas_info(gas: &GasInfo) -> cosmos_sdk_proto::cosmos::base::abci::v1beta1::GasInfo {
    cosmos_sdk_proto::cosmos::base::abci::v1beta1::GasInfo {
        gas_wanted: gas.gas_wanted,
        gas_used: gas.gas_used,
    }
}

pub fn encode_tx_result(
    response: &TxResponse,
) -> cosmos_sdk_proto::cosmos::base::abci::v1beta1::Result {
    let combined_data = msg_data_to_proto(response.data.clone());

    cosmos_sdk_proto::cosmos::base::abci::v1beta1::Result {
        data: combined_data,
        events: response
            .events
            .iter()
            .cloned()
            .flat_map(|e| e.into_iter().map(encode_cosmos_event))
            .collect(),
        log: "".to_string(),
    }
}

pub fn msg_data_to_proto(data: Vec<Vec<u8>>) -> Vec<u8> {
    let data = data
        .into_iter()
        .map(|d| cosmos_sdk_proto::cosmos::base::abci::v1beta1::MsgData {
            // TODO: what type?? do we need to pass this data everywhere in our MsgResult type?
            // This type used as a placeholder for now, so we don't get parse failure if someone tries
            // to decode this data (but data dropped)
            msg_type: cosmos_sdk_proto::cosmos::bank::v1beta1::MsgSend::TYPE_URL.to_string(),
            data: d,
        })
        .collect();

    cosmos_sdk_proto::cosmos::base::abci::v1beta1::TxMsgData { data }.encode_to_vec()
}

pub fn encode_cosmos_event(event: Event) -> cosmos_sdk_proto::tendermint::abci::Event {
    let attributes = event
        .attributes
        .into_iter()
        .map(|a| cosmos_sdk_proto::tendermint::abci::EventAttribute {
            key: a.key,
            value: a.value,
            index: true,
        })
        .collect();
    cosmos_sdk_proto::tendermint::abci::Event {
        r#type: event.ty,
        attributes,
    }
}
