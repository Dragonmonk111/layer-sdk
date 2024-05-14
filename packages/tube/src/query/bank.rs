use cosmwasm_std::Coin;

use cw_orch_core::environment::{BankQuerier, Querier};

pub struct Slay3rBank {}

impl Querier for Slay3rBank {
    type Error = slay3r_app::PulsarError;
}

impl BankQuerier for Slay3rBank {
    fn balance(
        &self,
        _address: impl Into<String>,
        _denom: Option<String>,
    ) -> Result<Vec<Coin>, Self::Error> {
        unimplemented!()
    }

    fn total_supply(&self) -> Result<Vec<Coin>, Self::Error> {
        unimplemented!()
    }

    fn supply_of(&self, _denom: impl Into<String>) -> Result<Coin, Self::Error> {
        unimplemented!()
    }
}
