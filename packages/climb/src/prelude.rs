// local "prelude" that isn't exported
pub(crate) use crate::{network::apply_grpc_height, proto_helpers::*};
pub(crate) use anyhow::{anyhow, bail, Context, Result};

// common types
pub use crate::{
    address::Address,
    config::{ChainConfig, ChainId},
    events::CosmosTxEvents,
    network::ChainConfigGrpcExt,
    querier::contract::ContractMessage,
    querier::{QueryClient, QueryRequest},
    signing::contract::{ExecuteParams, InstantiateParams, MigrateParams},
    signing::{key::KeySigner, SigningClient},
    transaction::{TxBuilder, TxSigner},
};

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
