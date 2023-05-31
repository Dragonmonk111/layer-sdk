use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum WasmError {
    #[error("Cannot transfer funds from another account")]
    Unauthorized,
}
