use cosmwasm_std::StdError;
use thiserror::Error;

pub type PulsarResult<T> = Result<T, PulsarError>;

#[derive(Error, Debug)]
pub enum PulsarError {
    #[error("{0}")]
    Std(#[from] StdError),
}
