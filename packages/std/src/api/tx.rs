use cosmwasm_std::Event;

use crate::GasMeter;

/// Response from one message, to be combined for TxResponse
#[derive(Debug)]
pub struct MsgResponse {
    pub data: Option<Vec<u8>>,
    pub events: Vec<Event>,
}

impl MsgResponse {
    pub fn new(events: Vec<Event>, data: Vec<u8>) -> Self {
        MsgResponse {
            events,
            data: Some(data),
        }
    }

    pub fn events(events: Vec<Event>) -> Self {
        MsgResponse { events, data: None }
    }
}

// We get the gas_used / gas_wanted from the gas meter (outside of scope)
// Errors get codespace = "pulsar", code = 1, log = err.to_string()
// Success get data and events
// One entry in data and events per message
pub struct TxResponse {
    pub data: Vec<Vec<u8>>,
    pub events: Vec<Vec<Event>>,
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

pub struct TxResult<E: std::error::Error> {
    pub gas: GasInfo,
    pub result: Result<TxResponse, E>,
}
