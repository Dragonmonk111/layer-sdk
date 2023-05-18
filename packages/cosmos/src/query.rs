use pulsar_std::response::{BankQueryResponse, QueryResponse};
use pulsar_std::{Query, QueryError};

use cosmos_sdk_proto::cosmos::bank::v1beta1::{
    QueryAllBalancesResponse, QueryBalanceResponse, QuerySupplyOfResponse,
};
use cosmos_sdk_proto::prost::Message;

use crate::utils::{encode_sdk_coin, encode_sdk_coins};

pub fn parse_cosmos_query(_path: &str, _data: &[u8]) -> Result<Query, QueryError> {
    todo!()
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
