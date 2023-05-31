mod account_id;
pub mod api;
mod encode;
mod gas;
mod msg;
mod pubkey;
mod query;
mod time;
mod tx;

pub use account_id::{must_id, AccountId, AccountIdError, DEFAULT_BECH32_PREFIX};
use cosmwasm_std::Binary;
pub use encode::{CoinEncode, HexEncode};
pub use gas::{GasError, GasMeter, GasResult};
pub use msg::{required_signer, BankMsg, Msg, MsgError, WasmMsg};
pub use pubkey::PubKey;
pub use query::{AuthQuery, BankQuery, Query, QueryError};
pub use time::{format_timestamp_rfc3339, Duration, Rfc3339, Timestamp};
pub use tx::{FeeInfo, SignedTx, SigningInfo, Tx, TxError};

pub mod response {
    pub use crate::query::{
        AccountResponse, AllBalanceResponse, AuthQueryResponse, BalanceResponse, BankQueryResponse,
        QueryResponse, SupplyResponse,
    };
}

pub fn binary_to_string(
    data: &Binary,
    fmt: &mut std::fmt::Formatter,
) -> Result<(), std::fmt::Error> {
    match std::str::from_utf8(data.as_slice()) {
        Ok(s) => fmt.write_str(s),
        Err(_) => write!(fmt, "{:?}", data),
    }
}
