use tracing::instrument;

use cosmwasm_std::to_vec;
// Convert from pulsar types into abci types
use pulsar_app::{PulsarError, PulsarResult};
use pulsar_cosmos::{encode_cosmos_response, msg_data_to_proto};

use crate::convert::{consensus_params_to_proto, events_to_proto, validator_updates_to_proto};

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
    height: u64,
) -> tendermint_proto::abci::ResponseQuery {
    match response {
        Ok(response) => {
            // TODO: error not unwrap
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
                height: height.try_into().unwrap(),
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
            height: height.try_into().unwrap(),
            codespace: "".to_string(),
        },
    }
}

#[instrument(skip_all, level = "trace")]
pub fn check_response_to_proto(
    response: pulsar_std::api::TxResult<PulsarError>,
) -> tendermint_proto::abci::ResponseCheckTx {
    let (gas_wanted, gas_used) = tx_gas_to_proto(response.gas);
    let (code, data, events, log) = tx_result_to_proto(response.result);

    tendermint_proto::abci::ResponseCheckTx {
        code,
        data: data.into(),
        log,
        info: "".to_string(),
        gas_wanted,
        gas_used,
        events,
        codespace: "".to_string(),
    }
}

// This has the same fields as tendermint_proto::abci::ResponseCheckTx but different name,
// so we make helper functions to do the same logic.
#[instrument(skip_all, level = "trace")]
pub fn tx_result_to_exec_tx_proto(
    response: pulsar_std::api::TxResult<PulsarError>,
) -> tendermint_proto::abci::ExecTxResult {
    let (gas_wanted, gas_used) = tx_gas_to_proto(response.gas);
    let (code, data, events, log) = tx_result_to_proto(response.result);

    tendermint_proto::abci::ExecTxResult {
        code,
        data: data.into(),
        log,
        info: "".to_string(),
        gas_wanted,
        gas_used,
        events,
        codespace: "".to_string(),
    }
}

#[instrument(skip_all, level = "trace")]
pub fn finalize_response_to_proto(
    response: pulsar_std::api::FinalizeBlockResponse<PulsarError>,
) -> tendermint_proto::abci::ResponseFinalizeBlock {
    let tx_results = response
        .tx_results
        .into_iter()
        .map(tx_result_to_exec_tx_proto)
        .collect();
    tendermint_proto::abci::ResponseFinalizeBlock {
        events: events_to_proto(response.events),
        tx_results,
        // TODO: implement these two maps
        validator_updates: validator_updates_to_proto(response.validator_updates),
        consensus_param_updates: response
            .consensus_param_updates
            .map(consensus_params_to_proto),
        app_hash: response.app_hash.into(),
    }
}

fn tx_gas_to_proto(gas: pulsar_std::api::GasInfo) -> (i64, i64) {
    (
        gas.gas_wanted.try_into().unwrap(),
        gas.gas_used.try_into().unwrap(),
    )
}

#[instrument(skip_all, level = "trace")]
fn tx_result_to_proto(
    result: PulsarResult<pulsar_std::api::TxResponse>,
) -> (u32, Vec<u8>, Vec<tendermint_proto::abci::Event>, String) {
    match result {
        Ok(resp) => {
            // FIXME: needed for compatibility but slow, review later
            let log =
                String::from_utf8(to_vec(&resp.events).unwrap()).unwrap_or_else(|e| e.to_string());
            let events = resp.events.into_iter().flat_map(events_to_proto).collect();
            let data = msg_data_to_proto(resp.data); // flatten
            (0, data, events, log)
        }
        Err(e) => (1, Vec::new(), Vec::new(), e.to_string()),
    }
}
