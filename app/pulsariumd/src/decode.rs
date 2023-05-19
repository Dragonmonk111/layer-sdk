// Convert from pulsar types into abci types

pub fn decode_init_response(
    _response: pulsar_std::api::InitChainResponse,
) -> tendermint_proto::abci::ResponseInitChain {
    todo!()
}
