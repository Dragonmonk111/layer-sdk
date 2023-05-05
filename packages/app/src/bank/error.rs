use thiserror::Error;

#[derive(Error, Debug)]
pub enum BankError {
    #[error("Account {0} has insufficient funds")]
    InsufficientFunds(String),

    #[error("Unsupported bank query: {0}")]
    UnsupportedQuery(String),

    #[error("Cannot transfer empty coins amount")]
    NoEmptyTransfer,
}
