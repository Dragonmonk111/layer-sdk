use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum IbcError {
    #[error("client already exists: {0}")]
    ClientExists(String),

    #[error("client not found: {0}")]
    ClientNotFound(String),

    #[error("connection already exists: {0}")]
    ConnectionExists(String),

    #[error("connection not found: {0}")]
    ConnectionNotFound(String),

    #[error("connection not open: {0}")]
    ConnectionNotOpen(String),

    #[error("channel already exists: {0}/{1}")]
    ChannelExists(String, String),

    #[error("channel not found: {0}/{1}")]
    ChannelNotFound(String, String),

    #[error("channel not open: {0}/{1}")]
    ChannelNotOpen(String, String),

    #[error("invalid state transition for {object}: expected {expected}, found {found}")]
    InvalidState {
        object: String,
        expected: String,
        found: String,
    },

    #[error("signer {signer} is not authorized for this message")]
    Unauthorized { signer: String },

    #[error("invalid packet commitment")]
    InvalidCommitment,

    #[error("{0}")]
    Invalid(String),
}
