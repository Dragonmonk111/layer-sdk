mod error;
mod keeper;

pub use error::AuthError;
pub use keeper::{fee_collector_account, parse_keys, Auth, TxData, NAMESPACE_AUTH};
