// Convert from abci types into pulsar types

use crate::convert::{
    consensus_params_from_proto, timestamp_from_proto, validator_updates_from_proto,
};

pub fn init_request_from_proto(
    request: tendermint_proto::abci::RequestInitChain,
) -> pulsar_std::api::InitChainRequest {
    pulsar_std::api::InitChainRequest {
        time: timestamp_from_proto(request.time.unwrap()),
        chain_id: request.chain_id,
        consensus_params: consensus_params_from_proto(request.consensus_params.unwrap()),
        validators: validator_updates_from_proto(request.validators),
        app_state: request.app_state_bytes.to_vec().into(),
        // set the height to 1 if not provided or set to 0 (Go zero means "default" means 1)
        initial_height: request.initial_height.max(1) as u64,
    }
}

pub fn query_request_from_proto(
    _request: tendermint_proto::abci::RequestQuery,
) -> pulsar_std::Query {
    todo!()
}

pub fn check_request_from_proto(
    _request: tendermint_proto::abci::RequestCheckTx,
) -> pulsar_std::Tx {
    todo!()
}

pub fn finalize_request_from_proto(
    _request: tendermint_proto::abci::RequestFinalizeBlock,
) -> pulsar_std::api::Block {
    todo!()
}
