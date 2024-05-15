use cosmwasm_std::{Coin, StdError};

use cw_orch_core::environment::{BankQuerier, Querier};

use slay3r_std::{
    response::{BankQueryResponse, QueryResponse},
    AccountId, BankQuery,
};

use crate::Slay3rTube;

pub struct Slay3rBank {
    tube: Slay3rTube,
}

impl Slay3rBank {
    pub fn new(tube: &Slay3rTube) -> Self {
        Self { tube: tube.clone() }
    }
}

impl Querier for Slay3rBank {
    type Error = slay3r_app::PulsarError;
}

impl BankQuerier for Slay3rBank {
    /// If denom is None, returns all balances
    fn balance(
        &self,
        address: impl Into<String>,
        denom: Option<String>,
    ) -> Result<Vec<Coin>, Self::Error> {
        let app = self.tube.app.borrow();
        let address = AccountId::parse_string(&address.into())?;

        if let Some(denom) = denom {
            let query = BankQuery::Balance { address, denom };
            let res = app.query(query.into())?;
            match res {
                QueryResponse::Bank(BankQueryResponse::Balance(r)) => Ok(vec![r.amount]),
                _ => Err(StdError::generic_err("unexpected response").into()),
            }
        } else {
            let query = BankQuery::AllBalances { address };
            let res = app.query(query.into())?;
            match res {
                QueryResponse::Bank(BankQueryResponse::AllBalances(r)) => Ok(r.amount),
                _ => Err(StdError::generic_err("unexpected response").into()),
            }
        }
    }

    fn total_supply(&self) -> Result<Vec<Coin>, Self::Error> {
        let app = self.tube.app.borrow();
        let query = BankQuery::TotalSupply {};
        let res = app.query(query.into())?;
        match res {
            QueryResponse::Bank(BankQueryResponse::TotalSupply(r)) => Ok(r.amounts),
            _ => Err(StdError::generic_err("unexpected response").into()),
        }
    }

    fn supply_of(&self, denom: impl Into<String>) -> Result<Coin, Self::Error> {
        let app = self.tube.app.borrow();
        let query = BankQuery::Supply {
            denom: denom.into(),
        };
        let res = app.query(query.into())?;
        match res {
            QueryResponse::Bank(BankQueryResponse::Supply(r)) => Ok(r.amount),
            _ => Err(StdError::generic_err("unexpected response").into()),
        }
    }
}
