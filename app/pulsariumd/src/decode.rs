// Convert from pulsar types into abci types
use pulsar_app::{PulsarError, PulsarResult};

pub fn decode_init_response(
    _response: pulsar_std::api::InitChainResponse,
) -> tendermint_proto::abci::ResponseInitChain {
    todo!()
}

pub fn decode_query_response(
    _response: PulsarResult<pulsar_std::response::QueryResponse<PulsarError>>,
) -> tendermint_proto::abci::ResponseQuery {
    todo!()
}

pub fn decode_check_response(
    _response: pulsar_std::api::TxResult<PulsarError>,
) -> tendermint_proto::abci::ResponseCheckTx {
    todo!()
}

pub fn decode_finalize_response(
    _response: pulsar_std::api::FinalizeBlockResponse<PulsarError>,
) -> tendermint_proto::abci::ResponseFinalizeBlock {
    todo!()
}
