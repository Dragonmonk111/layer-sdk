// Convert from abci types into pulsar types

pub fn encode_init_request(
    _request: tendermint_proto::abci::RequestInitChain,
) -> pulsar_std::api::InitChainRequest {
    todo!()
}
