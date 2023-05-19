// Convert from abci types into pulsar types

pub fn encode_init_request(
    _request: tendermint_proto::abci::RequestInitChain,
) -> pulsar_std::api::InitChainRequest {
    todo!()
}

pub fn encode_query_request(_request: tendermint_proto::abci::RequestQuery) -> pulsar_std::Query {
    todo!()
}

pub fn encode_check_request(_request: tendermint_proto::abci::RequestCheckTx) -> pulsar_std::Tx {
    todo!()
}

pub fn encode_finalize_request(
    _request: tendermint_proto::abci::RequestFinalizeBlock,
) -> pulsar_std::api::Block {
    todo!()
}
