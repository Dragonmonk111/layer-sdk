use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Binary, CosmosMsg, CustomMsg, CustomQuery};

/// This is a CustomMsg implementation for the layer-sdk chains
/// Can only be called by root contact
#[cw_serde]
pub enum CustomRootMsg {
    /// Call Sudo on any contract
    Sudo {
        contract_addr: String,
        /// msg is the json-encoded SudoMsg struct that will be passed to the new code
        // #[derivative(Debug(format_with = "crate::binary_to_string"))]
        msg: Binary,
    },
    /// Migrate any contract
    Migrate {
        contract_addr: String,
        /// the code_id of the new logic to place in the given contract
        new_code_id: u64,
        /// msg is the json-encoded MigrateMsg struct that will be passed to the new code
        // #[derivative(Debug(format_with = "crate::binary_to_string"))]
        msg: Binary,
    },
    /// Sets a new admin (for migrate) on the given contract.
    UpdateAdmin {
        contract_addr: String,
        admin: String,
    },
    /// Clears the admin on the given contract, so no more migration possible.
    ClearAdmin {
        contract_addr: String,
    },
    Pin {
        code_id: u64,
    },
    Unpin {
        code_id: u64,
    },
    // TODO: set tendermint validator set
}

impl CustomMsg for CustomRootMsg {}

impl From<CustomRootMsg> for CosmosMsg<CustomRootMsg> {
    fn from(msg: CustomRootMsg) -> Self {
        CosmosMsg::Custom(msg)
    }
}

/// This is a CustomQuery implementation for the layer-sdk chains.
#[cw_serde]
#[derive(QueryResponses)]
pub enum CustomRootQuery {
    // #[returns(Result)]
    // Request {}
}

impl CustomQuery for CustomRootQuery {}

/// This is sudo message that gets called on the root contract at various points in it's life cycle
#[cw_serde]
pub enum SudoMsg {
    BeginBlock {},
    EndBlock {},
}

#[cw_serde]
pub struct InstantiateMsg {
    /// This address is given the super power to assign privileges to other contracts.
    pub gov_address: String,
}

/// These are the valid execute messages on the layer contract
#[cw_serde]
pub enum ExecuteMsg {
    #[serde(untagged)]
    Gov(GovMsg),
    /// These are the ExecuteMsg variants that can be called by "system contracts"
    /// once "promoted" by the governance contract
    /// (which may also include the gov contract itself)
    #[serde(untagged)]
    System(SystemMsg),
}

pub type SystemMsg = CustomRootMsg;

/// These are the ExecuteMsg variants that can only be called by the gov address
#[cw_serde]
pub enum GovMsg {
    /// Hand off governance power to a new address
    ChangeGov { gov_address: String },
    PromoteContract {
        // TODO: we also want to provide partial priviledges
        contract_address: String,
    },
    DemoteContract {
        // TODO: we also want to provide partial priviledges
        contract_address: String,
    },
    SetBeginBlocker {
        contract_address: String,
        // TODO: gas limit
        // TODO: bool, halt on error (if true, chain halts on error result)
    },
    SetEndBlocker {
        contract_address: String,
        // TODO: gas limit
        // TODO: bool, halt on error (if true, chain halts on error result)
    },
}

/// TODO: these are the queries served by the root contract itself
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    // #[returns(Result)]
    // Request {}
}
