use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum WasmError {
    #[error("Cannot transfer funds from another account")]
    Unauthorized,

    #[error("Used an invalid key in an event attribute: {0}")]
    InvalidAttributeKey(String),

    #[error("VmError: {0}")]
    Vm(String),

    #[error("Contract Error: {0}")]
    Contract(String),
}
