pub(super) use anyhow::{anyhow, bail, Context, Result};

pub use crate::AddrString;

// helpers re-exported
pub use cosmos_sdk_proto::traits::Message;
pub use cosmrs::tx::MessageExt;

// Common types that can be confusing between different proto files.
// standardized here. In cases where we want helper methods, use extension traits
// so that we don't have to deal with confusion between types.
pub use cosmos_sdk_proto::cosmos::base::v1beta1::Coin;

// helper functions
pub fn new_coin(amount: impl ToString, denom: impl ToString) -> Coin {
    Coin {
        denom: denom.to_string(),
        amount: amount.to_string(),
    }
}
