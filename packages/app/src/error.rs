use cosmwasm_std::StdError;
use pulsar_std::{AccountIdError, GasError, QueryError, TxError};
use pulsar_storage::PlusError;
use thiserror::Error;

use crate::auth::AuthError;
use crate::bank::BankError;

pub type PulsarResult<T> = Result<T, PulsarError>;

#[derive(Error, Debug, PartialEq)]
pub enum PulsarError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Auth(#[from] AuthError),

    #[error("{0}")]
    AccountId(#[from] AccountIdError),

    #[error("{0}")]
    Bank(#[from] BankError),

    #[error("{0}")]
    Query(#[from] QueryError),

    #[error("{0}")]
    Tx(#[from] TxError),

    #[error("{0}")]
    Gas(#[from] GasError),
}

impl From<PlusError> for PulsarError {
    fn from(value: PlusError) -> Self {
        match value {
            PlusError::Gas(err) => PulsarError::Gas(err),
            PlusError::Std(err) => PulsarError::Std(err),
        }
    }
}
