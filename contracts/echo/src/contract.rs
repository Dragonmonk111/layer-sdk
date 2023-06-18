use cw_storage_plus::Item;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError};

use crate::msg::*;
use std::fmt::Debug;

const COUNTER: Item<u64> = Item::new("counter");

#[derive(thiserror::Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("{0}")]
    Echo(String),
}

fn incr_counter(deps: DepsMut) -> Result<u64, ContractError> {
    let mut counter = COUNTER.may_load(deps.storage)?.unwrap_or_default();
    counter += 1;
    COUNTER.save(deps.storage, &counter)?;
    Ok(counter)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    incr_counter(deps.branch())?;
    match msg {
        InstantiateMsg::Echo(EchoMsg {
            data,
            attrs,
            events,
        }) => {
            let mut res = Response::new();
            res.data = data;
            res.attributes = attrs;
            res.events = events;
            Ok(res)
        }
        InstantiateMsg::Fail { msg } => Err(ContractError::Echo(msg)),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    mut deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    incr_counter(deps.branch())?;
    match msg {
        ExecuteMsg::Echo(EchoMsg {
            data,
            attrs,
            events,
        }) => {
            let mut res = Response::new();
            res.data = data;
            res.attributes = attrs;
            res.events = events;
            Ok(res)
        }
        ExecuteMsg::Fail { msg } => Err(ContractError::Echo(msg)),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, StdError> {
    match msg {
        QueryMsg::Counter {} => {
            let count = COUNTER.load(deps.storage)?;
            to_binary(&CounterResponse { count })
        }
    }
}
