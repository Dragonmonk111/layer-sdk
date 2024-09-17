#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{ensure_eq, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, StdResult};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::msg::{CustomRootMsg, ExecuteMsg, GovMsg, InstantiateMsg, QueryMsg, SudoMsg, SystemMsg};
use crate::state::{CallBackInfo, BEGIN_BLOCKERS, END_BLOCKERS, GOV, SYSTEM};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:layer-root";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub type Response = cosmwasm_std::Response<CustomRootMsg>;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let gov = deps.api.addr_validate(&msg.gov_address)?;
    GOV.save(deps.storage, &gov)?;

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    // TODO: cw-utils::non_payable
    match msg {
        ExecuteMsg::Gov(gov_msg) => {
            let gov = GOV.load(deps.storage)?;
            ensure_eq!(info.sender, gov, ContractError::Unauthorized);
            match gov_msg {
                GovMsg::ChangeGov { gov_address } => {
                    execute_gov::change_gov(deps, env, gov_address)
                }
                GovMsg::PromoteContract { contract_address } => {
                    execute_gov::promote_contract(deps, env, contract_address)
                }
                GovMsg::DemoteContract { contract_address } => {
                    execute_gov::demote_contract(deps, env, contract_address)
                }
                GovMsg::SetBeginBlocker { contract_address } => {
                    execute_gov::set_begin_blocker(deps, env, contract_address)
                }
                GovMsg::SetEndBlocker { contract_address } => {
                    execute_gov::set_end_blocker(deps, env, contract_address)
                }
            }
        }
        ExecuteMsg::System(system_msg) => {
            let _system = SYSTEM
                .may_load(deps.storage, &info.sender)?
                .ok_or(ContractError::Unauthorized)?;
            // TODO: later we may check per-call permissions using this system info
            match system_msg {
                SystemMsg::ClearAdmin { contract_addr } => {
                    let _ = deps.api.addr_validate(&contract_addr)?;
                    let msg = CustomRootMsg::ClearAdmin { contract_addr };
                    Ok(Response::new().add_message(msg))
                }
                SystemMsg::UpdateAdmin {
                    contract_addr,
                    admin,
                } => {
                    let _ = deps.api.addr_validate(&contract_addr)?;
                    let _ = deps.api.addr_validate(&admin)?;
                    let msg = CustomRootMsg::UpdateAdmin {
                        contract_addr,
                        admin,
                    };
                    Ok(Response::new().add_message(msg))
                }
                SystemMsg::Pin { code_id } => {
                    let msg = CustomRootMsg::Pin { code_id };
                    Ok(Response::new().add_message(msg))
                }
                SystemMsg::Unpin { code_id } => {
                    let msg = CustomRootMsg::Unpin { code_id };
                    Ok(Response::new().add_message(msg))
                }
                SystemMsg::Sudo { contract_addr, msg } => {
                    let _ = deps.api.addr_validate(&contract_addr)?;
                    let msg = CustomRootMsg::Sudo { contract_addr, msg };
                    Ok(Response::new().add_message(msg))
                }
                SystemMsg::Migrate {
                    contract_addr,
                    new_code_id,
                    msg,
                } => {
                    let _ = deps.api.addr_validate(&contract_addr)?;
                    let msg = CustomRootMsg::Migrate {
                        contract_addr,
                        new_code_id,
                        msg,
                    };
                    Ok(Response::new().add_message(msg))
                }
            }
        }
    }
}

mod execute_gov {
    use crate::state::{CallBackInfo, Permissions, BEGIN_BLOCKERS, END_BLOCKERS};

    use super::*;

    pub fn change_gov(
        deps: DepsMut,
        _env: Env,
        gov_address: String,
    ) -> Result<Response, ContractError> {
        let gov_addr = deps.api.addr_validate(&gov_address)?;
        GOV.save(deps.storage, &gov_addr)?;

        let res = Response::new();
        Ok(res)
    }

    pub fn promote_contract(
        deps: DepsMut,
        _env: Env,
        contract_address: String,
    ) -> Result<Response, ContractError> {
        let contract_addr = deps.api.addr_validate(&contract_address)?;
        let permissions = Permissions {};
        SYSTEM.save(deps.storage, &contract_addr, &permissions)?;

        let res = Response::new();
        Ok(res)
    }

    pub fn demote_contract(
        deps: DepsMut,
        _env: Env,
        contract_address: String,
    ) -> Result<Response, ContractError> {
        let contract_addr = deps.api.addr_validate(&contract_address)?;
        SYSTEM.remove(deps.storage, &contract_addr);

        let res = Response::new();
        Ok(res)
    }

    pub fn set_begin_blocker(
        deps: DepsMut,
        _env: Env,
        contract_address: String,
    ) -> Result<Response, ContractError> {
        let contract_addr = deps.api.addr_validate(&contract_address)?;
        let mut blockers = BEGIN_BLOCKERS.load(deps.storage)?;

        if blockers.iter().any(|b| b.contract == contract_addr) {
            return Err(ContractError::AlreadyRegistred);
        }

        let callback = CallBackInfo {
            contract: contract_addr,
        };
        blockers.push(callback);
        BEGIN_BLOCKERS.save(deps.storage, &blockers)?;

        let res = Response::new();
        Ok(res)
    }

    pub fn set_end_blocker(
        deps: DepsMut,
        _env: Env,
        contract_address: String,
    ) -> Result<Response, ContractError> {
        let contract_addr = deps.api.addr_validate(&contract_address)?;
        let mut blockers = END_BLOCKERS.load(deps.storage)?;

        if blockers.iter().any(|b| b.contract == contract_addr) {
            return Err(ContractError::AlreadyRegistred);
        }

        let callback = CallBackInfo {
            contract: contract_addr,
        };
        blockers.push(callback);
        END_BLOCKERS.save(deps.storage, &blockers)?;

        let res = Response::new();
        Ok(res)
    }
}

mod execute_system {}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps, _env: Env, _msg: QueryMsg) -> StdResult<Binary> {
    unimplemented!()
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    match msg {
        SudoMsg::BeginBlock {} => {
            let msg = SudoMsg::BeginBlock {};
            let cbs = BEGIN_BLOCKERS.load(deps.storage)?;
            run_callbacks(deps, env, msg, cbs)
        }
        SudoMsg::EndBlock {} => {
            let msg = SudoMsg::EndBlock {};
            let cbs = END_BLOCKERS.load(deps.storage)?;
            run_callbacks(deps, env, msg, cbs)
        }
    }
}

fn run_callbacks(
    _deps: DepsMut,
    _env: Env,
    msg: SudoMsg,
    cbs: Vec<CallBackInfo>,
) -> Result<Response, ContractError> {
    let msg = to_json_binary(&msg)?;
    let mut res = Response::new();
    for cb in cbs {
        let msg = CustomRootMsg::Sudo {
            contract_addr: cb.contract.to_string(),
            msg: msg.clone(),
        };
        res = res.add_message(msg);
    }
    Ok(res)
}

#[cfg(test)]
mod tests {}
