// Convert from abci types into pulsar types

use pulsar_cosmos::{parse_cosmos_query, parse_cosmos_tx};

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
        // initial_height: request.initial_height.max(1) as u64,
        initial_height: request.initial_height as u64,
    }
}

pub fn query_request_from_proto(
    request: tendermint_proto::abci::RequestQuery,
    // we need to pass in out-of-bound info for simulate
    chain_id: &str,
) -> pulsar_std::Query {
    // TODO: no panic
    if request.prove {
        panic!("Proofs not supported");
    }
    if request.height > 0 {
        panic!("Height not supported");
    }
    // TODO: error not unwrap
    parse_cosmos_query(&request.path, &request.data, chain_id).unwrap()
}

pub fn check_request_from_proto(
    request: &tendermint_proto::abci::RequestCheckTx,
    chain_id: &str,
) -> pulsar_std::Tx {
    // TODO: error not unwrap
    parse_cosmos_tx(&request.tx, chain_id).unwrap()
}

pub fn finalize_request_from_proto(
    request: tendermint_proto::abci::RequestFinalizeBlock,
    chain_id: &str,
) -> pulsar_std::api::Block {
    let txs = request
        .txs
        .into_iter()
        .map(|tx| parse_cosmos_tx(&tx, chain_id).unwrap())
        .collect();
    let last_votes = match request.decided_last_commit {
        None => vec![],
        Some(commit) => commit
            .votes
            .into_iter()
            .filter_map(|vote| {
                vote.validator.map(|v| pulsar_std::api::Validator {
                    address: v.address.into(),
                    power: v.power.try_into().unwrap(),
                })
            })
            .collect(),
    };
    pulsar_std::api::Block {
        txs,
        height: request.height.try_into().unwrap(),
        time: timestamp_from_proto(request.time.unwrap()),
        proposer_address: request.proposer_address.into(),
        last_votes,
    }
}
