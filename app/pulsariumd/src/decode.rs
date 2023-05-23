// Convert from pulsar types into abci types
use pulsar_app::{PulsarError, PulsarResult};
use pulsar_cosmos::{encode_cosmos_response, msg_data_to_proto};

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

fn tx_gas_to_proto(gas: pulsar_std::api::GasInfo) -> (i64, i64) {
    (
        gas.gas_wanted.try_into().unwrap(),
        gas.gas_used.try_into().unwrap(),
    )
}

fn tx_result_to_proto(
    result: PulsarResult<pulsar_std::api::TxResponse>,
) -> (u32, Vec<u8>, Vec<tendermint_proto::abci::Event>, String) {
    match result {
        Ok(resp) => {
            let events = resp.events.into_iter().flat_map(events_to_proto).collect();
            let data = msg_data_to_proto(resp.data); // flatten
            (0, data, events, "".to_string())
        }
        Err(e) => (1, Vec::new(), Vec::new(), e.to_string()),
    }
}

fn events_to_proto(event: Vec<cosmwasm_std::Event>) -> Vec<tendermint_proto::abci::Event> {
    event.into_iter().map(event_to_proto).collect()
}

pub fn event_to_proto(event: cosmwasm_std::Event) -> tendermint_proto::abci::Event {
    let attributes = event
        .attributes
        .into_iter()
        .map(|a| tendermint_proto::abci::EventAttribute {
            key: a.key,
            value: a.value,
            index: true,
        })
        .collect();
    tendermint_proto::abci::Event {
        r#type: event.ty,
        attributes,
    }
}

/*
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ResponseCheckTx {
    #[prost(uint32, tag = "1")]
    pub code: u32,
    #[prost(bytes = "bytes", tag = "2")]
    pub data: ::prost::bytes::Bytes,
    /// nondeterministic
    #[prost(string, tag = "3")]
    pub log: ::prost::alloc::string::String,
    /// nondeterministic
    #[prost(string, tag = "4")]
    pub info: ::prost::alloc::string::String,
    #[prost(int64, tag = "5")]
    pub gas_wanted: i64,
    #[prost(int64, tag = "6")]
    pub gas_used: i64,
    #[prost(message, repeated, tag = "7")]
    pub events: ::prost::alloc::vec::Vec<Event>,
    #[prost(string, tag = "8")]
    pub codespace: ::prost::alloc::string::String,
}
 */

pub fn finalize_response_to_proto(
    _response: pulsar_std::api::FinalizeBlockResponse<PulsarError>,
) -> tendermint_proto::abci::ResponseFinalizeBlock {
    todo!()
}

/*
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ExecTxResult {
    #[prost(uint32, tag = "1")]
    pub code: u32,
    #[prost(bytes = "bytes", tag = "2")]
    pub data: ::prost::bytes::Bytes,
    /// nondeterministic
    #[prost(string, tag = "3")]
    pub log: ::prost::alloc::string::String,
    /// nondeterministic
    #[prost(string, tag = "4")]
    pub info: ::prost::alloc::string::String,
    #[prost(int64, tag = "5")]
    pub gas_wanted: i64,
    #[prost(int64, tag = "6")]
    pub gas_used: i64,
    /// nondeterministic
    #[prost(message, repeated, tag = "7")]
    pub events: ::prost::alloc::vec::Vec<Event>,
    #[prost(string, tag = "8")]
    pub codespace: ::prost::alloc::string::String,
}

 */
