use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};

/// The one gov contract
pub const GOV: Item<Addr> = Item::new("gov");

/// List of system contracts
pub const SYSTEM: Map<&Addr, Permissions> = Map::new("system");

/// Which contracts are called on BeginBlock (no Map, as we want to maintain order)
pub const BEGIN_BLOCKERS: Item<Vec<CallBackInfo>> = Item::new("begin_block");

/// Which contracts are called on EndBlock (no Map, as we want to maintain order)
pub const END_BLOCKERS: Item<Vec<CallBackInfo>> = Item::new("end_block");

#[cw_serde]
pub struct Permissions {
    // TODO: some info so it is not all-or-none
}

#[cw_serde]
pub struct CallBackInfo {
    pub contract: Addr,
    // TODO: some info on gas limits, panics, etc
}
