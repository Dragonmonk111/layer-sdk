use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Attribute, Binary, Event, ReplyOn};

#[cw_serde]
pub struct EchoMsg {
    pub data: Option<Binary>,
    pub attrs: Vec<Attribute>,
    pub events: Vec<Event>,
}

#[cw_serde]
pub struct CallInfo {
    /// how we handle replies (if we catch error, we stop failure)
    pub reply_on: ReplyOn,
    /// if set, we override parent data with our own data in reply
    pub override_data: bool,
    /// set a gas limit on the submsg
    pub gas_limit: Option<u64>,
}

#[cw_serde]
pub struct InstantiateMsg {
    /// The code_id we wish to instantiate (of echo)
    pub code_id: u64,
    /// The instantiate msg we wish to pass to the echo contract
    pub msg: tc_echo::InstantiateMsg,
    /// how to handle the submsg
    pub subcall: CallInfo,
}

#[cw_serde]
pub struct ExecuteMsg {
    /// The instantiate msg we wish to pass to the echo contract
    pub msg: tc_echo::ExecuteMsg,
    /// how to handle the submsg
    pub subcall: CallInfo,
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(CounterResponse)]
    Counter {},
}

#[cw_serde]
pub struct CounterResponse {
    pub calls: u64,
    pub replies: u64,
}
