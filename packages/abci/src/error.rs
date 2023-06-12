use thiserror::Error;

use crate::server::ConnectionType;

#[derive(Error, Debug)]
pub enum AbciError {
    #[error("{0}")]
    Io(#[from] tokio::io::Error),

    #[error("{0}")]
    Encode(#[from] prost::EncodeError),

    #[error("{0}")]
    Decode(#[from] prost::DecodeError),

    #[error("Rejecting message {message} from {connection:?}")]
    InvalidMessage {
        message: &'static str,
        connection: ConnectionType,
    },
}
