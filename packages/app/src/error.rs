use cosmwasm_std::StdError;
use pulsar_std::{QueryError, TxError};
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
    Bank(#[from] BankError),

    #[error("{0}")]
    Query(#[from] QueryError),

    #[error("{0}")]
    Tx(#[from] TxError),
}
