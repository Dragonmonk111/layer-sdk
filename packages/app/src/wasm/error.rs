use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum WasmError {
    #[error("Cannot transfer funds from another account")]
    Unauthorized,

    #[error("Used an invalid key in an event attribute: {0}")]
    InvalidAttributeKey(String),
}
