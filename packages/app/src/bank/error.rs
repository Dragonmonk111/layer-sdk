use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum BankError {
    // FIXME: use this for send rather than the generic underflow/overflow error
    #[error("Account {0} has insufficient funds")]
    InsufficientFunds(String),

    #[error("Unsupported bank query: {0}")]
    UnsupportedQuery(String),

    #[error("Cannot transfer empty coins amount")]
    NoEmptyTransfer,

    #[error("Cannot transfer funds from another account")]
    Unauthorized,

    #[error("Bank amount contains the same denomination twice: {0}")]
    DuplicateDenom(String),

    #[error("Initializing bank account on existing account: {0}")]
    ReinitializeExistingAccount(String),
}
