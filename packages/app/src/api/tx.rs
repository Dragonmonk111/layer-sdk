use cosmwasm_std::Event;

use crate::error::PulsarError;
use pulsar_std::GasMeter;

// We get the gas_used / gas_wanted from the gas meter (outside of scope)
// Errors get codespace = "pulsar", code = 1, log = err.to_string()
// Success get data and events
#[derive(Debug)]
pub struct TxResponse {
    pub data: Option<Vec<u8>>,
    pub events: Vec<Event>,
}

impl TxResponse {
    pub fn new(events: Vec<Event>, data: Vec<u8>) -> Self {
        TxResponse {
            events,
            data: Some(data),
        }
    }

    pub fn events(events: Vec<Event>) -> Self {
        TxResponse { events, data: None }
    }
}

#[derive(Debug, Default)]
pub struct GasInfo {
    pub gas_used: u64,
    pub gas_wanted: u64,
}

impl GasInfo {
    pub fn from_meter(meter: &GasMeter) -> Self {
        GasInfo {
            gas_used: meter.used(),
            gas_wanted: meter.limit(),
        }
    }
}

pub struct TxResult {
    pub gas: GasInfo,
    pub result: Result<TxResponse, PulsarError>,
}

// Note: we may want to use custom event type to support index bool???
/*
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EventAttribute {
    #[prost(string, tag = "1")]
    pub key: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub value: ::prost::alloc::string::String,
    /// nondeterministic
    #[prost(bool, tag = "3")]
    pub index: bool,
}
*/
