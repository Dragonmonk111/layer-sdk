use cosmwasm_std::{Coin, StdError};
use std::error::Error as Err;
use std::fmt::{Display, Formatter};
use thiserror::Error;

use crate::account_id::{AccountId, AccountIdError};
use crate::api::TxResult;
use crate::tx::Tx;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    /// Return the raw binary value stored under that key
    Raw {
        key: Vec<u8>,
    },
    Auth(AuthQuery),
    Bank(BankQuery),
    Simulate(Tx),
}

impl From<AuthQuery> for Query {
    fn from(value: AuthQuery) -> Self {
        Query::Auth(value)
    }
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
pub enum AuthQuery {
    /// Return value is of type AccountResponse.
    Account { address: AccountId },
}

impl Display for AuthQuery {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthQuery::Account { .. } => f.write_str("AuthQuery::Account"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryResponse<E: Err> {
    Raw { key: Vec<u8>, value: Vec<u8> },
    Auth(AuthQueryResponse),
    Bank(BankQueryResponse),
    Simulate(TxResult<E>),
}

impl<E: Err> From<AuthQueryResponse> for QueryResponse<E> {
    fn from(value: AuthQueryResponse) -> Self {
        QueryResponse::Auth(value)
    }
}

impl<E: Err> From<BankQueryResponse> for QueryResponse<E> {
    fn from(value: BankQueryResponse) -> Self {
        QueryResponse::Bank(value)
    }
}

impl<E: Err> From<TxResult<E>> for QueryResponse<E> {
    fn from(value: TxResult<E>) -> Self {
        QueryResponse::Simulate(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthQueryResponse {
    Account(AccountResponse),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountResponse {
    /// This is External Account in Ethereum terms, controlled by a public key
    External {
        address: AccountId,
        pubkey: Option<crate::PubKey>,
        sequence: u64,
    },
    /// No pubkey can control this, either contract or "module account"
    Internal { address: AccountId },
    /// Used for account abstraction, where a contract can validate what a pubkey can do
    Smart {
        address: AccountId,
        // FIXME: any more info to add here?
        contract: AccountId,
    },
}

impl<E: Err> From<AccountResponse> for QueryResponse<E> {
    fn from(value: AccountResponse) -> Self {
        AuthQueryResponse::Account(value).into()
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

impl<E: Err> From<SupplyResponse> for QueryResponse<E> {
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

impl<E: Err> From<BalanceResponse> for QueryResponse<E> {
    fn from(value: BalanceResponse) -> Self {
        BankQueryResponse::Balance(value).into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllBalanceResponse {
    /// Returns all non-zero coins held by this account.
    pub amount: Vec<Coin>,
}

impl<E: Err> From<AllBalanceResponse> for QueryResponse<E> {
    fn from(value: AllBalanceResponse) -> Self {
        BankQueryResponse::AllBalances(value).into()
    }
}

#[derive(Error, Debug, PartialEq)]
pub enum QueryError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unsupported path: {0}")]
    UnsupportedPath(String),

    #[error("{0}")]
    Addr(#[from] AccountIdError),

    /// FIXME: either ensure all callers of this function produce determinstic strings,
    /// Or remove all info
    #[error("Parse: {0}")]
    ParseError(String),

    /// FIXME: either ensure all callers of this function produce determinstic strings,
    /// Or remove all info
    #[error("Encoding: {0}")]
    EncodingError(String),
}
