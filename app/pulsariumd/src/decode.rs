// Convert from pulsar types into abci types
use pulsar_app::{PulsarError, PulsarResult};
use pulsar_cosmos::encode_cosmos_response;

use crate::convert::{consensus_params_to_proto, validator_updates_to_proto};

pub fn init_response_to_proto(
    response: pulsar_std::api::InitChainResponse,
) -> tendermint_proto::abci::ResponseInitChain {
    tendermint_proto::abci::ResponseInitChain {
        consensus_params: Some(consensus_params_to_proto(response.consensus_params)),
        validators: validator_updates_to_proto(response.validators),
        app_hash: response.app_hash.into(),
    }
}

pub fn query_response_to_proto(
    response: PulsarResult<pulsar_std::response::QueryResponse<PulsarError>>,
) -> tendermint_proto::abci::ResponseQuery {
    match response {
        Ok(response) => {
            // TODO: remove unwrap
            let value = encode_cosmos_response(&response).unwrap();
            let key = match response {
                pulsar_std::response::QueryResponse::Raw { key, .. } => key,
                _ => Vec::new(),
            };
            tendermint_proto::abci::ResponseQuery {
                code: 0,
                log: "".to_string(),
                info: "".to_string(),
                index: 0,
                key: key.into(),
                value: value.into(),
                proof_ops: None,
                height: 0,
                codespace: "".to_string(),
            }
        }
        Err(err) => tendermint_proto::abci::ResponseQuery {
            code: 1,
            log: err.to_string(),
            info: "".to_string(),
            index: 0,
            key: Vec::new().into(),
            value: Vec::new().into(),
            proof_ops: None,
            height: 0,
            codespace: "".to_string(),
        },
    }
}

pub fn check_response_to_proto(
    _response: pulsar_std::api::TxResult<PulsarError>,
) -> tendermint_proto::abci::ResponseCheckTx {
    todo!()
}

pub fn finalize_response_to_proto(
    _response: pulsar_std::api::FinalizeBlockResponse<PulsarError>,
) -> tendermint_proto::abci::ResponseFinalizeBlock {
    todo!()
}
