use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Invalid Signature")]
    InvalidSignature,
}
