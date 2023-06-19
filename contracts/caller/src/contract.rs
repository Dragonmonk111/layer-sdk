use cw_storage_plus::Item;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, CosmosMsg, Deps, DepsMut, Env, Event, MessageInfo, Reply, Response,
    StdError, SubMsg, WasmMsg,
};
use cw_utils::{parse_execute_response_data, parse_instantiate_response_data, ParseReplyError};

use crate::msg::*;
use core::panic;
use std::fmt::Debug;

const CALLS: Item<u64> = Item::new("calls");
const REPLIES: Item<u64> = Item::new("replies");
const ECHO: Item<String> = Item::new("echo");

#[derive(thiserror::Error, Debug)]
pub enum ContractError {
    #[error("Std: {0}")]
    Std(#[from] StdError),

    #[error("Parse: {0}")]
    Parse(#[from] ParseReplyError),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("{0}")]
    Echo(String),
}

fn incr_calls(deps: DepsMut) -> Result<u64, ContractError> {
    let mut counter = CALLS.may_load(deps.storage)?.unwrap_or_default();
    counter += 1;
    CALLS.save(deps.storage, &counter)?;
    Ok(counter)
}

fn incr_replies(deps: DepsMut) -> Result<u64, ContractError> {
    let mut counter = REPLIES.may_load(deps.storage)?.unwrap_or_default();
    counter += 1;
    REPLIES.save(deps.storage, &counter)?;
    Ok(counter)
}

const INIT_IGNORE_DATA: u64 = 2;
const INIT_SET_DATA: u64 = 3;
const EXEC_IGNORE_DATA: u64 = 4;
const EXEC_SET_DATA: u64 = 5;

fn build_submsg(msg: impl Into<CosmosMsg>, info: CallInfo, is_init: bool) -> SubMsg {
    let id = match (is_init, info.override_data) {
        (true, true) => INIT_SET_DATA,
        (true, false) => INIT_IGNORE_DATA,
        (false, true) => EXEC_SET_DATA,
        (false, false) => EXEC_IGNORE_DATA,
    };
    SubMsg {
        id,
        msg: msg.into(),
        gas_limit: info.gas_limit,
        reply_on: info.reply_on,
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    incr_calls(deps.branch())?;
    let init = WasmMsg::Instantiate {
        admin: Some(env.contract.address.into()),
        code_id: msg.code_id,
        msg: to_binary(&msg.msg)?,
        funds: vec![],
        label: "My best friend".to_string(),
    };
    let sub = build_submsg(init, msg.subcall, true);
    let event = Event::new("instantiate").add_attribute("code_id", msg.code_id.to_string());
    let res = Response::new()
        .add_submessage(sub)
        .add_event(event)
        .set_data(b"init");
    Ok(res)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    mut deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    incr_calls(deps.branch())?;
    let echo_addr = ECHO.load(deps.storage)?;
    let exec = WasmMsg::Execute {
        msg: to_binary(&msg.msg)?,
        funds: vec![],
        contract_addr: echo_addr.clone(),
    };
    let sub = build_submsg(exec, msg.subcall, false);
    let event = Event::new("execute").add_attribute("contract", echo_addr);
    let res = Response::new()
        .add_submessage(sub)
        .add_event(event)
        .set_data(b"exec");
    Ok(res)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, StdError> {
    match msg {
        QueryMsg::Counter {} => {
            let calls = CALLS.load(deps.storage)?;
            let replies = REPLIES.load(deps.storage)?;
            to_binary(&CounterResponse { calls, replies })
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(mut deps: DepsMut, _env: Env, reply: Reply) -> Result<Response, ContractError> {
    incr_replies(deps.branch())?;
    let res = match reply.result.into_result() {
        Err(e) => {
            let event = Event::new("reply").add_attribute("error", e);
            Response::new().add_event(event)
        }
        Ok(r) => match reply.id {
            INIT_IGNORE_DATA => {
                // empty reply, we just want to get the address
                let init_data = parse_instantiate_response_data(&r.data.unwrap())?;
                ECHO.save(deps.storage, &init_data.contract_address)?;
                Response::new()
            }
            INIT_SET_DATA => {
                // empty reply, we just want to get the address
                let init_data = parse_instantiate_response_data(&r.data.unwrap())?;
                ECHO.save(deps.storage, &init_data.contract_address)?;
                if let Some(data) = init_data.data {
                    Response::new().set_data(data)
                } else {
                    Response::new()
                }
            }
            EXEC_SET_DATA => {
                // empty reply, we just want to get the address
                let exec_data = parse_execute_response_data(&r.data.unwrap())?;
                if let Some(data) = exec_data.data {
                    Response::new().set_data(data)
                } else {
                    Response::new()
                }
            }
            EXEC_IGNORE_DATA => Response::new(),
            _ => panic!("unexpected reply id: {}", reply.id),
        },
    };
    Ok(res)
}
