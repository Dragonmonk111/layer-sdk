use crate::addr::{Addr, AddrError};
use cosmwasm_std::Coin;
use std::fmt::{Display, Formatter};
use thiserror::Error;

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
    Balance { address: Addr, denom: String },
    /// Note that this may be much more expensive than Balance and should be avoided if possible.
    /// Return value is AllBalanceResponse.
    AllBalances { address: Addr },
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

pub struct SupplyResponse {
    /// Always returns a Coin with the requested denom.
    /// This will be of zero amount if the denom does not exist.
    pub amount: Coin,
}

pub struct BalanceResponse {
    /// Always returns a Coin with the requested denom.
    /// This may be of 0 amount if no such funds.
    pub amount: Coin,
}

pub struct AllBalanceResponse {
    /// Returns all non-zero coins held by this account.
    pub amount: Vec<Coin>,
}

#[derive(Error, Debug)]
pub enum QueryError {
    #[error("Unsupported path: {0}")]
    UnsupportedPath(String),

    #[error("{0}")]
    Addr(#[from] AddrError),
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
