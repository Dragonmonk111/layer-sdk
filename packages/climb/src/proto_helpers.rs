use anyhow::Result;

pub fn msg_into_google_any<M>(msg: &M) -> Result<tendermint_proto::google::protobuf::Any>
where
    M: cosmrs::proto::prost::Name,
{
    msg_into_cosmrs_any(msg).map(|any| tendermint_proto::google::protobuf::Any {
        type_url: any.type_url,
        value: any.value,
    })
}

pub fn msg_into_cosmrs_any<M>(msg: &M) -> Result<cosmrs::Any>
where
    M: cosmrs::proto::prost::Name,
{
    cosmrs::Any::from_msg(msg).map_err(|e| e.into())
}

pub fn msg_layer_into_cosmrs_any<M>(type_url: String, msg: &M) -> Result<cosmrs::Any>
where
    M: cosmrs::tx::MessageExt
{

    let mut value = Vec::new();
    cosmrs::proto::prost::Message::encode(msg, &mut value)?;
    Ok(cosmrs::Any { type_url, value })
}