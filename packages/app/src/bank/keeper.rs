use itertools::Itertools;

use cosmwasm_std::{coin, ensure_eq, BlockInfo, Coin, Event, Storage, Uint128};
use cw_storage_plus::Map;
use cw_utils::NativeBalance;

use pulsar_std::response::{AllBalanceResponse, BalanceResponse, QueryResponse, SupplyResponse};
use pulsar_std::{AccountId, BankMsg, BankQuery, GasMeter};
use pulsar_storage::{prefixed, prefixed_read};

use crate::api::TxResponse;
use crate::bank::BankError;
use crate::error::{PulsarError, PulsarResult};
use crate::sm::StateMachine;

// store supply for each denom
const SUPPLY: Map<&str, Uint128> = Map::new("supply");
// FIXME: store each denom separate - (&Addr, &str), Uint128
const BALANCES: Map<&AccountId, NativeBalance> = Map::new("balances");

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
        account: &AccountId,
        amount: Vec<Coin>,
    ) -> PulsarResult<()> {
        let mut bank_storage = prefixed(storage, NAMESPACE_BANK);
        self.set_balance(&mut bank_storage, account, amount)
    }

    fn set_balance(
        &self,
        bank_storage: &mut dyn Storage,
        account: &AccountId,
        amount: Vec<Coin>,
    ) -> PulsarResult<()> {
        let mut balance = NativeBalance(amount);
        balance.normalize();

        // update the supply for each coin
        // TODO: this assume account had no balance before... let's see how to do this proper
        for coin in balance.0.iter() {
            SUPPLY.update::<_, PulsarError>(bank_storage, &coin.denom, |supply| {
                Ok(supply.unwrap_or_default() + coin.amount)
            })?;
        }

        // store user balance
        BALANCES
            .save(bank_storage, account, &balance)
            .map_err(Into::into)
    }

    fn get_balance(
        &self,
        bank_storage: &dyn Storage,
        account: &AccountId,
    ) -> PulsarResult<Vec<Coin>> {
        let val = BALANCES.may_load(bank_storage, account)?;
        Ok(val.unwrap_or_default().into_vec())
    }

    fn get_supply(&self, bank_storage: &dyn Storage, denom: &str) -> PulsarResult<Uint128> {
        let val = SUPPLY.may_load(bank_storage, denom)?;
        Ok(val.unwrap_or_default())
    }

    fn send(
        &self,
        bank_storage: &mut dyn Storage,
        from_address: AccountId,
        to_address: AccountId,
        amount: Vec<Coin>,
    ) -> PulsarResult<()> {
        self.burn(bank_storage, from_address, amount.clone())?;
        self.mint(bank_storage, to_address, amount)
    }

    fn mint(
        &self,
        bank_storage: &mut dyn Storage,
        to_address: AccountId,
        amount: Vec<Coin>,
    ) -> PulsarResult<()> {
        let amount = self.normalize_amount(amount)?;

        // update the supply for each coin
        // TODO: this assume account had no balance before... let's see how to do this proper
        for coin in &amount {
            SUPPLY.update::<_, PulsarError>(bank_storage, &coin.denom, |supply| {
                Ok(supply.unwrap_or_default() + coin.amount)
            })?;
        }

        let b = self.get_balance(bank_storage, &to_address)?;
        let b = NativeBalance(b) + NativeBalance(amount);
        self.set_balance(bank_storage, &to_address, b.into_vec())
    }

    fn burn(
        &self,
        bank_storage: &mut dyn Storage,
        from_address: AccountId,
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
    // helper to move funds when called from another module
    pub fn transfer(
        &self,
        storage: &mut dyn Storage,
        from_address: AccountId,
        to_address: AccountId,
        amount: Vec<Coin>,
    ) -> PulsarResult<()> {
        let mut bank_storage = prefixed(storage, NAMESPACE_BANK);
        self.send(&mut bank_storage, from_address, to_address, amount)
    }

    // TODO add: BlockInfo, &StateMachine (for callbacks)
    pub fn process_msg(
        &self,
        storage: &mut dyn Storage,
        _meter: &mut GasMeter,
        _block: &BlockInfo,
        _sm: &StateMachine,
        signer: &AccountId,
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
    pub fn query(
        &self,
        storage: &dyn Storage,
        _meter: &mut GasMeter,
        _block: &BlockInfo,
        _sm: &StateMachine,
        request: BankQuery,
    ) -> PulsarResult<QueryResponse> {
        let bank_storage = prefixed_read(storage, NAMESPACE_BANK);
        match request {
            BankQuery::AllBalances { address } => {
                let amount = self.get_balance(&bank_storage, &address)?;
                let res = AllBalanceResponse { amount };
                Ok(res.into())
            }
            BankQuery::Balance { address, denom } => {
                let all_amounts = self.get_balance(&bank_storage, &address)?;
                let amount = all_amounts
                    .into_iter()
                    .find(|c| c.denom == denom)
                    .unwrap_or_else(|| coin(0, denom));
                let res = BalanceResponse { amount };
                Ok(res.into())
            }
            BankQuery::Supply { denom } => {
                let amount = self.get_supply(&bank_storage, &denom)?;
                let res = SupplyResponse {
                    amount: Coin { denom, amount },
                };
                Ok(res.into())
            }
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
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::{coins, StdError};
    use pulsar_std::response::BankQueryResponse;
    use pulsar_storage::MemoryStorage;

    fn query_balance(bank: &Bank, store: &dyn Storage, rcpt: &AccountId) -> Vec<Coin> {
        let req = BankQuery::AllBalances {
            address: rcpt.clone(),
        };
        let block = mock_env().block;
        let mut meter = GasMeter::new(500_000);
        let sm = StateMachine::default();

        let resp = bank.query(store, &mut meter, &block, &sm, req).unwrap();
        match resp {
            QueryResponse::Bank(BankQueryResponse::AllBalances(AllBalanceResponse { amount })) => {
                amount
            }
            _ => panic!("unexpected return"),
        }
    }

    #[test]
    fn get_set_balance() {
        let mut store = MemoryStorage::new();
        let block = mock_env().block;
        let sm = StateMachine::new();
        let mut meter = GasMeter::new(500_000);

        let owner = AccountId::unchecked("owner");
        let rcpt = AccountId::unchecked("receiver");
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
            address: owner.clone(),
        };
        let resp = bank.query(&store, &mut meter, &block, &sm, req).unwrap();
        match resp {
            QueryResponse::Bank(BankQueryResponse::AllBalances(AllBalanceResponse { amount })) => {
                assert_eq!(amount, norm)
            }
            _ => panic!("unexpected return"),
        }

        let req = BankQuery::AllBalances {
            address: rcpt.clone(),
        };
        let resp = bank.query(&store, &mut meter, &block, &sm, req).unwrap();
        match resp {
            QueryResponse::Bank(BankQueryResponse::AllBalances(AllBalanceResponse { amount })) => {
                assert_eq!(amount, vec![])
            }
            _ => panic!("unexpected return"),
        }

        let req = BankQuery::Balance {
            address: owner.clone(),
            denom: "eth".into(),
        };
        let resp = bank.query(&store, &mut meter, &block, &sm, req).unwrap();
        match resp {
            QueryResponse::Bank(BankQueryResponse::Balance(BalanceResponse { amount })) => {
                assert_eq!(amount, coin(100, "eth"))
            }
            _ => panic!("unexpected return"),
        }

        let req = BankQuery::Balance {
            address: owner,
            denom: "foobar".into(),
        };
        let resp = bank.query(&store, &mut meter, &block, &sm, req).unwrap();
        match resp {
            QueryResponse::Bank(BankQueryResponse::Balance(BalanceResponse { amount })) => {
                assert_eq!(amount, coin(0, "foobar"))
            }
            _ => panic!("unexpected return"),
        }

        let req = BankQuery::Balance {
            address: rcpt,
            denom: "eth".into(),
        };
        let resp = bank.query(&store, &mut meter, &block, &sm, req).unwrap();
        match resp {
            QueryResponse::Bank(BankQueryResponse::Balance(BalanceResponse { amount })) => {
                assert_eq!(amount, coin(0, "eth"))
            }
            _ => panic!("unexpected return"),
        }

        let req = BankQuery::Supply {
            denom: "eth".into(),
        };
        let resp = bank.query(&store, &mut meter, &block, &sm, req).unwrap();
        match resp {
            QueryResponse::Bank(BankQueryResponse::Supply(SupplyResponse { amount })) => {
                assert_eq!(amount, coin(100, "eth"))
            }
            _ => panic!("unexpected return"),
        }

        let req = BankQuery::Supply {
            denom: "foobar".into(),
        };
        let resp = bank.query(&store, &mut meter, &block, &sm, req).unwrap();
        match resp {
            QueryResponse::Bank(BankQueryResponse::Supply(SupplyResponse { amount })) => {
                assert_eq!(amount, coin(0, "foobar"))
            }
            _ => panic!("unexpected return"),
        }
    }

    #[test]
    fn send_coins() {
        let mut store = MemoryStorage::new();
        let mut meter = GasMeter::new(1_000_000);
        let block = mock_env().block;
        let sm = StateMachine::new();

        let owner = AccountId::unchecked("owner");
        let rcpt = AccountId::unchecked("receiver");
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
        bank.process_msg(&mut store, &mut meter, &block, &sm, &owner, msg.clone())
            .unwrap();
        let rich = query_balance(&bank, &store, &owner);
        assert_eq!(vec![coin(15, "btc"), coin(70, "eth")], rich);
        let poor = query_balance(&bank, &store, &rcpt);
        assert_eq!(vec![coin(10, "btc"), coin(30, "eth")], poor);

        // cannot send from someone else's account
        let err = bank
            .process_msg(&mut store, &mut meter, &block, &sm, &rcpt, msg)
            .unwrap_err();
        assert_eq!(err, PulsarError::Bank(BankError::Unauthorized));

        // cannot send too much
        let msg = BankMsg::Send {
            sender: owner.clone(),
            recipient: rcpt.clone(),
            amount: coins(20, "btc"),
        };
        bank.process_msg(&mut store, &mut meter, &block, &sm, &owner, msg)
            .unwrap_err();

        let rich = query_balance(&bank, &store, &owner);
        assert_eq!(vec![coin(15, "btc"), coin(70, "eth")], rich);
    }

    #[test]
    fn burn_coins() {
        let mut store = MemoryStorage::new();
        let block = mock_env().block;
        let mut meter = GasMeter::new(1_000_000);
        let sm = StateMachine::new();

        let owner = AccountId::unchecked("owner");
        let rcpt = AccountId::unchecked("recipient");
        let init_funds = vec![coin(20, "btc"), coin(100, "eth")];

        // set money
        let bank = Bank::new();
        bank.init_balance(&mut store, &owner, init_funds).unwrap();

        // burn both tokens
        let to_burn = vec![coin(30, "eth"), coin(5, "btc")];
        let msg = BankMsg::Burn {
            sender: owner.clone(),
            amount: to_burn,
        };
        bank.process_msg(&mut store, &mut meter, &block, &sm, &owner, msg)
            .unwrap();
        let rich = query_balance(&bank, &store, &owner);
        assert_eq!(vec![coin(15, "btc"), coin(70, "eth")], rich);

        // cannot burn too much
        let msg = BankMsg::Burn {
            sender: owner.clone(),
            amount: coins(20, "btc"),
        };
        let err = bank
            .process_msg(&mut store, &mut meter, &block, &sm, &owner, msg)
            .unwrap_err();
        assert!(matches!(err, PulsarError::Std(StdError::Overflow { .. })));

        let rich = query_balance(&bank, &store, &owner);
        assert_eq!(vec![coin(15, "btc"), coin(70, "eth")], rich);

        // cannot burn from empty account
        let msg = BankMsg::Burn {
            sender: rcpt.clone(),
            amount: coins(1, "btc"),
        };
        let err = bank
            .process_msg(&mut store, &mut meter, &block, &sm, &rcpt, msg)
            .unwrap_err();
        assert!(matches!(err, PulsarError::Std(StdError::Overflow { .. })));
    }

    #[test]
    fn fail_on_zero_values() {
        let mut store = MemoryStorage::new();
        let mut meter = GasMeter::new(1_000_000);
        let block = mock_env().block;
        let sm = StateMachine::new();

        let owner = AccountId::unchecked("owner");
        let rcpt = AccountId::unchecked("recipient");
        let init_funds = vec![coin(5000, "atom"), coin(100, "eth")];

        // set money
        let bank = Bank::new();
        bank.init_balance(&mut store, &owner, init_funds).unwrap();

        // can send normal amounts
        let msg = BankMsg::Send {
            sender: owner.clone(),
            recipient: rcpt.clone(),
            amount: coins(100, "atom"),
        };
        bank.process_msg(&mut store, &mut meter, &block, &sm, &owner, msg)
            .unwrap();

        // fails send on no coins
        let msg = BankMsg::Send {
            sender: owner.clone(),
            recipient: rcpt.clone(),
            amount: vec![],
        };
        let err = bank
            .process_msg(&mut store, &mut meter, &block, &sm, &owner, msg)
            .unwrap_err();
        assert_eq!(err, PulsarError::Bank(BankError::NoEmptyTransfer));

        // fails send on 0 coins
        let msg = BankMsg::Send {
            sender: owner.clone(),
            recipient: rcpt.clone(),
            amount: coins(0, "atom"),
        };
        let err = bank
            .process_msg(&mut store, &mut meter, &block, &sm, &owner, msg)
            .unwrap_err();
        assert_eq!(err, PulsarError::Bank(BankError::NoEmptyTransfer));

        // fails burn on no coins
        let msg = BankMsg::Burn {
            sender: owner.clone(),
            amount: vec![],
        };
        let err = bank
            .process_msg(&mut store, &mut meter, &block, &sm, &owner, msg)
            .unwrap_err();
        assert_eq!(err, PulsarError::Bank(BankError::NoEmptyTransfer));

        // fails burn on 0 coins
        let msg = BankMsg::Burn {
            sender: owner.clone(),
            amount: coins(0, "atom"),
        };
        let err = bank
            .process_msg(&mut store, &mut meter, &block, &sm, &owner, msg)
            .unwrap_err();
        assert_eq!(err, PulsarError::Bank(BankError::NoEmptyTransfer));

        // can mint
        let mut bank_storage = prefixed(&mut store, NAMESPACE_BANK);
        bank.mint(&mut bank_storage, rcpt.clone(), coins(4321, "atom"))
            .unwrap();

        // mint fails with 0 tokens
        let err = bank
            .mint(&mut bank_storage, rcpt.clone(), coins(0, "atom"))
            .unwrap_err();
        assert_eq!(err, PulsarError::Bank(BankError::NoEmptyTransfer));

        // mint fails with no tokens
        let err = bank.mint(&mut bank_storage, rcpt, vec![]).unwrap_err();
        assert_eq!(err, PulsarError::Bank(BankError::NoEmptyTransfer));
    }
}
