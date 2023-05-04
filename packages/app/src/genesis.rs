use cosmwasm_schema::cw_serde;
use cosmwasm_std::{from_slice, Coin};

use crate::error::PulsarError;

#[cw_serde]
pub struct GenesisState {
    pub bank: Vec<BankAccount>,
}

#[cw_serde]
pub struct BankAccount {
    pub address: String,
    pub balance: Vec<Coin>,
}

impl GenesisState {
    pub fn parse(data: &[u8]) -> Result<Self, PulsarError> {
        from_slice(data).map_err(Into::into)
    }
}
