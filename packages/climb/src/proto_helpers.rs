use crate::prelude::*;

// helpers re-exported
pub use cosmos_sdk_proto::prost::Message;
pub use cosmrs::tx::MessageExt;

/// the typical type used for turning protobuf messages into `Any` messages
/// especially used in transactions, and needed for multi-message transactions
/// so exported in the prelude
pub fn proto_into_any<M>(msg: &M) -> Result<cosmrs::Any>
where
    M: cosmrs::proto::prost::Name,
{
    cosmrs::Any::from_msg(msg).map_err(|e| e.into())
}

/// Internal helper for dealing with different `Any` types
/// ideally we can get rid of this, see https://github.com/informalsystems/tendermint-rs/issues/1462
pub fn msg_into_google_any<M>(msg: &M) -> Result<proto::Any>
where
    M: cosmrs::proto::prost::Name,
{
    proto_into_any(msg).map(|any| proto::Any {
        type_url: any.type_url,
        value: any.value,
    })
}
