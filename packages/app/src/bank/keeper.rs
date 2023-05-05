use itertools::Itertools;

use cosmwasm_std::{
    coin, ensure_eq, to_vec, AllBalanceResponse, BalanceResponse, Coin, Event, Storage,
};
use cw_storage_plus::Map;
use cw_utils::NativeBalance;

use pulsar_std::{Addr, BankMsg, BankQuery, GasMeter};
use pulsar_storage::{prefixed, prefixed_read};

use crate::api::TxResponse;
use crate::bank::BankError;
use crate::error::PulsarResult;

const BALANCES: Map<&Addr, NativeBalance> = Map::new("balances");

pub const NAMESPACE_BANK: &[u8] = b"bank";

#[derive(Default)]
pub struct Bank {}

impl Bank {
    pub fn new() -> Self {
        Bank {}
    }

    // this is an "admin" function to let us adjust bank accounts in genesis
    pub fn init_balance(
        &self,
        storage: &mut dyn Storage,
        account: &Addr,
        amount: Vec<Coin>,
    ) -> PulsarResult<()> {
        let mut bank_storage = prefixed(storage, NAMESPACE_BANK);
        self.set_balance(&mut bank_storage, account, amount)
    }

    fn set_balance(
        &self,
        bank_storage: &mut dyn Storage,
        account: &Addr,
        amount: Vec<Coin>,
    ) -> PulsarResult<()> {
        let mut balance = NativeBalance(amount);
        balance.normalize();
        BALANCES
            .save(bank_storage, account, &balance)
            .map_err(Into::into)
    }

    // this is an "admin" function to let us adjust bank accounts
    fn get_balance(&self, bank_storage: &dyn Storage, account: &Addr) -> PulsarResult<Vec<Coin>> {
        let val = BALANCES.may_load(bank_storage, account)?;
        Ok(val.unwrap_or_default().into_vec())
    }

    fn send(
        &self,
        bank_storage: &mut dyn Storage,
        from_address: Addr,
        to_address: Addr,
        amount: Vec<Coin>,
    ) -> PulsarResult<()> {
        self.burn(bank_storage, from_address, amount.clone())?;
        self.mint(bank_storage, to_address, amount)
    }

    fn mint(
        &self,
        bank_storage: &mut dyn Storage,
        to_address: Addr,
        amount: Vec<Coin>,
    ) -> PulsarResult<()> {
        let amount = self.normalize_amount(amount)?;
        let b = self.get_balance(bank_storage, &to_address)?;
        let b = NativeBalance(b) + NativeBalance(amount);
        self.set_balance(bank_storage, &to_address, b.into_vec())
    }

    fn burn(
        &self,
        bank_storage: &mut dyn Storage,
        from_address: Addr,
        amount: Vec<Coin>,
    ) -> PulsarResult<()> {
        let amount = self.normalize_amount(amount)?;
        let a = self.get_balance(bank_storage, &from_address)?;
        let a = (NativeBalance(a) - amount)?;
        self.set_balance(bank_storage, &from_address, a.into_vec())
    }

    /// Filters out all 0 value coins and returns an error if the resulting Vec is empty
    fn normalize_amount(&self, amount: Vec<Coin>) -> PulsarResult<Vec<Coin>> {
        let res: Vec<_> = amount.into_iter().filter(|x| !x.amount.is_zero()).collect();
        if res.is_empty() {
            Err(BankError::NoEmptyTransfer.into())
        } else {
            Ok(res)
        }
    }
}

impl Bank {
    // TODO add: BlockInfo, &StateMachine (for callbacks)
    pub fn process_msg(
        &self,
        storage: &mut dyn Storage,
        _meter: &mut GasMeter,
        signer: &Addr,
        msg: BankMsg,
    ) -> PulsarResult<TxResponse> {
        let mut bank_storage = prefixed(storage, NAMESPACE_BANK);
        match msg {
            BankMsg::Send {
                sender,
                recipient,
                amount,
            } => {
                ensure_eq!(signer, &sender, BankError::Unauthorized);
                let events = vec![Event::new("transfer")
                    .add_attribute("recipient", &recipient)
                    .add_attribute("sender", &sender)
                    .add_attribute("amount", coins_to_string(&amount))];
                self.send(&mut bank_storage, sender, recipient, amount)?;
                Ok(TxResponse::events(events))
            }
            BankMsg::Burn { sender, amount } => {
                ensure_eq!(signer, &sender, BankError::Unauthorized);
                let events = vec![Event::new("burn")
                    .add_attribute("sender", &sender)
                    .add_attribute("amount", coins_to_string(&amount))];
                self.burn(&mut bank_storage, sender, amount)?;
                Ok(TxResponse::events(events))
            }
        }
    }

    // TODO add: BlockInfo, &StateMachine (for callbacks)
    pub fn query(&self, storage: &dyn Storage, request: BankQuery) -> PulsarResult<Vec<u8>> {
        let bank_storage = prefixed_read(storage, NAMESPACE_BANK);
        match request {
            BankQuery::AllBalances { address } => {
                let amount = self.get_balance(&bank_storage, &address)?;
                let res = AllBalanceResponse { amount };
                Ok(to_vec(&res)?)
            }
            BankQuery::Balance { address, denom } => {
                let all_amounts = self.get_balance(&bank_storage, &address)?;
                let amount = all_amounts
                    .into_iter()
                    .find(|c| c.denom == denom)
                    .unwrap_or_else(|| coin(0, denom));
                let res = BalanceResponse { amount };
                Ok(to_vec(&res)?)
            }
            q => Err(BankError::UnsupportedQuery(q.to_string()).into()),
        }
    }
}

fn coins_to_string(coins: &[Coin]) -> String {
    coins
        .iter()
        .map(|c| format!("{}{}", c.amount, c.denom))
        .join(",")
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::error::PulsarError;
    use cosmwasm_std::testing::{mock_env, MockApi, MockQuerier, MockStorage};
    use cosmwasm_std::{coins, from_slice, Empty};

    fn query_balance(bank: &Bank, store: &dyn Storage, rcpt: &Addr) -> Vec<Coin> {
        let req = BankQuery::AllBalances {
            address: rcpt.clone().into(),
        };
        let block = mock_env().block;
        let querier: MockQuerier<Empty> = MockQuerier::new(&[]);

        let raw = bank.query(store, req).unwrap();
        let res: AllBalanceResponse = from_slice(&raw).unwrap();
        res.amount
    }

    #[test]
    fn get_set_balance() {
        let api = MockApi::default();
        let mut store = MockStorage::new();
        let block = mock_env().block;
        let querier: MockQuerier<Empty> = MockQuerier::new(&[]);

        let owner = Addr::unchecked("owner");
        let rcpt = Addr::unchecked("receiver");
        let init_funds = vec![coin(100, "eth"), coin(20, "btc")];
        let norm = vec![coin(20, "btc"), coin(100, "eth")];

        // set money
        let bank = Bank::new();
        bank.init_balance(&mut store, &owner, init_funds).unwrap();
        let bank_storage = prefixed_read(&store, NAMESPACE_BANK);

        // get balance work
        let rich = bank.get_balance(&bank_storage, &owner).unwrap();
        assert_eq!(rich, norm);
        let poor = bank.get_balance(&bank_storage, &rcpt).unwrap();
        assert_eq!(poor, vec![]);

        // proper queries work
        let req = BankQuery::AllBalances {
            address: owner.clone().into(),
        };
        let raw = bank.query(&api, req).unwrap();
        let res: AllBalanceResponse = from_slice(&raw).unwrap();
        assert_eq!(res.amount, norm);

        let req = BankQuery::AllBalances {
            address: rcpt.clone().into(),
        };
        let raw = bank.query(&api, req).unwrap();
        let res: AllBalanceResponse = from_slice(&raw).unwrap();
        assert_eq!(res.amount, vec![]);

        let req = BankQuery::Balance {
            address: owner.clone().into(),
            denom: "eth".into(),
        };
        let raw = bank.query(&api, req).unwrap();
        let res: BalanceResponse = from_slice(&raw).unwrap();
        assert_eq!(res.amount, coin(100, "eth"));

        let req = BankQuery::Balance {
            address: owner.into(),
            denom: "foobar".into(),
        };
        let raw = bank.query(&api, req).unwrap();
        let res: BalanceResponse = from_slice(&raw).unwrap();
        assert_eq!(res.amount, coin(0, "foobar"));

        let req = BankQuery::Balance {
            address: rcpt.into(),
            denom: "eth".into(),
        };
        let raw = bank.query(&api, req).unwrap();
        let res: BalanceResponse = from_slice(&raw).unwrap();
        assert_eq!(res.amount, coin(0, "eth"));
    }

    #[test]
    fn send_coins() {
        let api = MockApi::default();
        let mut store = MockStorage::new();
        let block = mock_env().block;
        let router = MockRouter::default();

        let owner = Addr::unchecked("owner");
        let rcpt = Addr::unchecked("receiver");
        let mut meter = GasMeter::new(1_000_000);
        let init_funds = vec![coin(20, "btc"), coin(100, "eth")];
        let rcpt_funds = vec![coin(5, "btc")];

        // set money
        let bank = Bank::new();
        bank.init_balance(&mut store, &owner, init_funds).unwrap();
        bank.init_balance(&mut store, &rcpt, rcpt_funds).unwrap();

        // send both tokens
        let to_send = vec![coin(30, "eth"), coin(5, "btc")];
        let msg = BankMsg::Send {
            sender: owner.clone(),
            recipient: rcpt.clone(),
            amount: to_send,
        };
        bank.process_msg(&mut store, &mut meter, &owner, msg.clone())
            .unwrap();
        let rich = query_balance(&bank, &store, &owner);
        assert_eq!(vec![coin(15, "btc"), coin(70, "eth")], rich);
        let poor = query_balance(&bank, &store, &rcpt);
        assert_eq!(vec![coin(10, "btc"), coin(30, "eth")], poor);

        // cannot send from someone else's account
        let err = bank
            .process_msg(&mut store, &mut meter, &rcpt, msg)
            .unwrap_err();
        assert_eq!(err, PulsarError::Bank(BankError::Unauthorized));

        // cannot send too much
        let msg = BankMsg::Send {
            sender: owner.clone(),
            recipient: rcpt.clone(),
            amount: coins(20, "btc"),
        };
        bank.process_msg(&mut store, &mut meter, &owner, msg)
            .unwrap_err();

        let rich = query_balance(&bank, &store, &owner);
        assert_eq!(vec![coin(15, "btc"), coin(70, "eth")], rich);
    }

    /*
    #[test]
    fn burn_coins() {
        let api = MockApi::default();
        let mut store = MockStorage::new();
        let block = mock_env().block;
        let router = MockRouter::default();

        let owner = Addr::unchecked("owner");
        let rcpt = Addr::unchecked("recipient");
        let init_funds = vec![coin(20, "btc"), coin(100, "eth")];

        // set money
        let bank = Bank::new();
        bank.init_balance(&mut store, &owner, init_funds).unwrap();

        // burn both tokens
        let to_burn = vec![coin(30, "eth"), coin(5, "btc")];
        let msg = BankMsg::Burn { amount: to_burn };
        bank.execute(&api, &mut store, &router, &block, owner.clone(), msg)
            .unwrap();
        let rich = query_balance(&bank, &api, &store, &owner);
        assert_eq!(vec![coin(15, "btc"), coin(70, "eth")], rich);

        // cannot burn too much
        let msg = BankMsg::Burn {
            amount: coins(20, "btc"),
        };
        let err = bank
            .execute(&api, &mut store, &router, &block, owner.clone(), msg)
            .unwrap_err();
        assert!(matches!(err.downcast().unwrap(), StdError::Overflow { .. }));

        let rich = query_balance(&bank, &api, &store, &owner);
        assert_eq!(vec![coin(15, "btc"), coin(70, "eth")], rich);

        // cannot burn from empty account
        let msg = BankMsg::Burn {
            amount: coins(1, "btc"),
        };
        let err = bank
            .execute(&api, &mut store, &router, &block, rcpt, msg)
            .unwrap_err();
        assert!(matches!(err.downcast().unwrap(), StdError::Overflow { .. }));
    }

    #[test]
    fn fail_on_zero_values() {
        let api = MockApi::default();
        let mut store = MockStorage::new();
        let block = mock_env().block;
        let router = MockRouter::default();

        let owner = Addr::unchecked("owner");
        let rcpt = Addr::unchecked("recipient");
        let init_funds = vec![coin(5000, "atom"), coin(100, "eth")];

        // set money
        let bank = Bank::new();
        bank.init_balance(&mut store, &owner, init_funds).unwrap();

        // can send normal amounts
        let msg = BankMsg::Send {
            to_address: rcpt.to_string(),
            amount: coins(100, "atom"),
        };
        bank.execute(&api, &mut store, &router, &block, owner.clone(), msg)
            .unwrap();

        // fails send on no coins
        let msg = BankMsg::Send {
            to_address: rcpt.to_string(),
            amount: vec![],
        };
        bank.execute(&api, &mut store, &router, &block, owner.clone(), msg)
            .unwrap_err();

        // fails send on 0 coins
        let msg = BankMsg::Send {
            to_address: rcpt.to_string(),
            amount: coins(0, "atom"),
        };
        bank.execute(&api, &mut store, &router, &block, owner.clone(), msg)
            .unwrap_err();

        // fails burn on no coins
        let msg = BankMsg::Burn { amount: vec![] };
        bank.execute(&api, &mut store, &router, &block, owner.clone(), msg)
            .unwrap_err();

        // fails burn on 0 coins
        let msg = BankMsg::Burn {
            amount: coins(0, "atom"),
        };
        bank.execute(&api, &mut store, &router, &block, owner, msg)
            .unwrap_err();

        // can mint via sudo
        let msg = BankSudo::Mint {
            to_address: rcpt.to_string(),
            amount: coins(4321, "atom"),
        };
        bank.sudo(&api, &mut store, &router, &block, msg).unwrap();

        // mint fails with 0 tokens
        let msg = BankSudo::Mint {
            to_address: rcpt.to_string(),
            amount: coins(0, "atom"),
        };
        bank.sudo(&api, &mut store, &router, &block, msg)
            .unwrap_err();

        // mint fails with no tokens
        let msg = BankSudo::Mint {
            to_address: rcpt.to_string(),
            amount: vec![],
        };
        bank.sudo(&api, &mut store, &router, &block, msg)
            .unwrap_err();
    }
    */
}
