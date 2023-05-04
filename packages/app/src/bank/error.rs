use thiserror::Error;

#[derive(Error, Debug)]
pub enum BankError {
    #[error("Account {0} has insufficient funds")]
    InsufficientFunds(String),
}
