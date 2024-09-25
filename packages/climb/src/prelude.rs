// local "prelude" that isn't exported
pub(crate) use crate::{network::apply_grpc_height, proto_helpers::*};
pub(crate) use anyhow::{anyhow, bail, Context, Result};

// common types
pub use crate::{
    address::Address,
    config::{ChainConfig, ChainId},
    contract_helpers::contract_str_to_msg,
    events::CosmosTxEvents,
    proto,
    proto::Coin,
    proto_helpers::proto_into_any,
    querier::{QueryClient, QueryRequest},
    signing::{key::KeySigner, SigningClient},
    transaction::{TxBuilder, TxSigner},
};

// Common types that can be confusing between different proto files.
// standardized here. In cases where we want helper methods, use extension traits
// so that we don't have to deal with confusion between types.

/// helper function to create a Coin
pub fn new_coin(amount: impl ToString, denom: impl ToString) -> proto::Coin {
    proto::Coin {
        denom: denom.to_string(),
        amount: amount.to_string(),
    }
}

/// helper function to create a vec of coins from an iterator of tuples
/// where the first is the amount, and the second is the denom.
/// Example:
/// ```ignore
/// use layer_climb::prelude::*;
///
/// new_coins([
///     ("uusd", "100"),
///     ("uslay", "200")
/// ])
/// ```
pub fn new_coins(
    coins: impl IntoIterator<Item = (impl ToString, impl ToString)>,
) -> Vec<proto::Coin> {
    coins
        .into_iter()
        .map(|(amount, denom)| new_coin(amount, denom))
        .collect()
}
