use cosmwasm_std::Coin;
use std::fmt::{Display, Formatter};
use thiserror::Error;

use crate::account_id::{AccountId, AccountIdError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    /// Return the raw binary value stored under that key
    Raw {
        key: Vec<u8>,
    },
    Bank(BankQuery),
}

impl From<BankQuery> for Query {
    fn from(value: BankQuery) -> Self {
        Query::Bank(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BankQuery {
    /// Return value is of type SupplyResponse.
    Supply { denom: String },
    /// Return value is BalanceResponse
    Balance { address: AccountId, denom: String },
    /// Note that this may be much more expensive than Balance and should be avoided if possible.
    /// Return value is AllBalanceResponse.
    AllBalances { address: AccountId },
}

impl Display for BankQuery {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BankQuery::Supply { .. } => f.write_str("BankQuery::Supply"),
            BankQuery::Balance { .. } => f.write_str("BankQuery::Balance"),
            BankQuery::AllBalances { .. } => f.write_str("BankQuery::AllBalances"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryResponse {
    Raw { value: Vec<u8> },
    Bank(BankQueryResponse),
}

impl From<BankQueryResponse> for QueryResponse {
    fn from(value: BankQueryResponse) -> Self {
        QueryResponse::Bank(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BankQueryResponse {
    Supply(SupplyResponse),
    Balance(BalanceResponse),
    AllBalances(AllBalanceResponse),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplyResponse {
    /// Always returns a Coin with the requested denom.
    /// This will be of zero amount if the denom does not exist.
    pub amount: Coin,
}

impl From<SupplyResponse> for QueryResponse {
    fn from(value: SupplyResponse) -> Self {
        BankQueryResponse::Supply(value).into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BalanceResponse {
    /// Always returns a Coin with the requested denom.
    /// This may be of 0 amount if no such funds.
    pub amount: Coin,
}

impl From<BalanceResponse> for QueryResponse {
    fn from(value: BalanceResponse) -> Self {
        BankQueryResponse::Balance(value).into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllBalanceResponse {
    /// Returns all non-zero coins held by this account.
    pub amount: Vec<Coin>,
}

impl From<AllBalanceResponse> for QueryResponse {
    fn from(value: AllBalanceResponse) -> Self {
        BankQueryResponse::AllBalances(value).into()
    }
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum QueryError {
    #[error("Unsupported path: {0}")]
    UnsupportedPath(String),

    #[error("{0}")]
    Addr(#[from] AccountIdError),
}

mod cosmos {
    use super::*;

    impl QueryResponse {
        /// Make a binary protobuf encoding of this type
        pub fn to_cosmos(&self) -> Result<Vec<u8>, QueryError> {
            todo!();
        }
    }
}

// mod cosmos {
//     use cosmos_sdk_proto::{
//         cosmos::bank::v1beta1::MsgSend,
//         cosmos::base::v1beta1::Coin as SdkCoin,
//         prost::DecodeError,
//         traits::{MessageExt, TypeUrl},
//     };
//
//     use cosmrs::Any;
// }
