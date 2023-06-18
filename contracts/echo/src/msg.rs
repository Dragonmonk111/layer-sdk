use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Attribute, Binary, Event};

#[cw_serde]
pub struct EchoMsg {
    pub data: Option<Binary>,
    pub attrs: Vec<Attribute>,
    pub events: Vec<Event>,
}

#[cw_serde]
pub enum InstantiateMsg {
    Echo(EchoMsg),
    Fail { msg: String },
}

#[cw_serde]
pub enum ExecuteMsg {
    Echo(EchoMsg),
    Fail { msg: String },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(CounterResponse)]
    Counter {},
}

#[cw_serde]
pub struct CounterResponse {
    pub count: u64,
}
