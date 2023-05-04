use cosmwasm_schema::cw_serde;
use cosmwasm_std::Coin;

#[cw_serde]
pub struct GenesisState {
    pub bank: Vec<BankAccount>,
}

#[cw_serde]
pub struct BankAccount {
    pub address: String,
    pub balance: Vec<Coin>,
}
