use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Binary, CustomMsg, CustomQuery};

use crate::AccountId;

/// This is a CustomMsg implementation for the layer-sdk chains
/// Can only be called by root contact
#[cw_serde]
pub enum CustomRootMsg {
    /// Call Sudo on any contract
    Sudo {
        contract_addr: AccountId,
        /// msg is the json-encoded SudoMsg struct that will be passed to the new code
        // #[derivative(Debug(format_with = "crate::binary_to_string"))]
        msg: Binary,
    },
    /// Migrate any contract
    Migrate {
        contract_addr: AccountId,
        /// the code_id of the new logic to place in the given contract
        new_code_id: u64,
        /// msg is the json-encoded MigrateMsg struct that will be passed to the new code
        // #[derivative(Debug(format_with = "crate::binary_to_string"))]
        msg: Binary,
    },
    /// Sets a new admin (for migrate) on the given contract.
    UpdateAdmin {
        contract_addr: AccountId,
        admin: AccountId,
    },
    /// Clears the admin on the given contract, so no more migration possible.
    ClearAdmin {
        contract_addr: AccountId,
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
pub enum LayerSudoMsg {
    BeginBlock {},
    EndBlock {},
}
