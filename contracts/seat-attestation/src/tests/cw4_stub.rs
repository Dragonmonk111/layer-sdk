//! Minimal cw4-group stand-in for multitest — implements just the
//! `member` / `list_members` query surface the seat-attestation contract
//! consumes, with a configurable (and mutable) member set.

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    to_json_binary, Binary, Deps, DepsMut, Empty, Env, MessageInfo, Order, Response, StdResult,
};
use cw_storage_plus::{Bound, Map};

pub const MEMBERS: Map<String, u64> = Map::new("members");

#[cw_serde]
pub struct InstantiateMsg {
    /// (addr, weight) pairs; weight 0 entries are skipped.
    pub members: Vec<(String, u64)>,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Set weights; weight 0 removes the member.
    UpdateMembers { members: Vec<(String, u64)> },
}

#[cw_serde]
pub enum QueryMsg {
    Member { addr: String },
    ListMembers {
        start_after: Option<String>,
        limit: Option<u32>,
    },
}

#[cw_serde]
pub struct MemberResponse {
    pub weight: Option<u64>,
}

#[cw_serde]
pub struct Member {
    pub addr: String,
    pub weight: u64,
}

#[cw_serde]
pub struct MemberListResponse {
    pub members: Vec<Member>,
}

pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<Empty>, cosmwasm_std::StdError> {
    for (addr, weight) in msg.members {
        if weight > 0 {
            MEMBERS.save(deps.storage, addr, &weight)?;
        }
    }
    Ok(Response::new())
}

pub fn execute(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<Empty>, cosmwasm_std::StdError> {
    match msg {
        ExecuteMsg::UpdateMembers { members } => {
            for (addr, weight) in members {
                if weight > 0 {
                    MEMBERS.save(deps.storage, addr, &weight)?;
                } else {
                    MEMBERS.remove(deps.storage, addr);
                }
            }
            Ok(Response::new())
        }
    }
}

pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Member { addr } => to_json_binary(&MemberResponse {
            weight: MEMBERS.may_load(deps.storage, addr)?,
        }),
        QueryMsg::ListMembers { start_after, limit } => {
            let limit = limit.unwrap_or(30) as usize;
            let start = start_after.map(Bound::exclusive);
            let members = MEMBERS
                .range(deps.storage, start, None, Order::Ascending)
                .take(limit)
                .map(|item| item.map(|(addr, weight)| Member { addr, weight }))
                .collect::<StdResult<Vec<_>>>()?;
            to_json_binary(&MemberListResponse { members })
        }
    }
}
