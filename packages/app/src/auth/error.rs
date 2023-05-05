use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum AuthError {
    #[error("Invalid Signature")]
    InvalidSignature,
}
