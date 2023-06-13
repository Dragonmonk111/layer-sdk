use cosmwasm_std::{coin, coins, to_binary, to_vec, Uint128};
use cw20::Cw20Coin;
use pulsar_std::{AccountId, WasmMsg};

use crate::{
    genesis::{BankAccount, GenesisState, WasmParams},
    testing::utils::*,
};

// v1.0.1
const CW20_BASE: &[u8] = include_bytes!("../../fixtures/cw20_base.wasm");

fn cw20_genesis(account: &AccountId) -> GenesisState {
    GenesisState {
        bank: vec![BankAccount {
            address: account.to_string(),
            balance: coins(100_000_000, "upulsar"),
        }],
        wasm: WasmParams {
            gov_account: account.to_string(),
        },
    }
}

fn query_cw20_balance(app: &TestApp, contract: &AccountId, account: &AccountId) -> Uint128 {
    let msg = cw20::Cw20QueryMsg::Balance {
        address: account.to_string(),
    };
    let cw20::BalanceResponse { balance } = app.query_wasm(contract, &msg).unwrap();
    balance
}

#[test]
fn happy_path_cw20() {
    let signer = PrivateKey::random();
    let sender = signer.to_pubkey().account_id().unwrap();
    let rcpt = AccountId::unchecked("getting paid");

    let path = prepare_cache("/tmp/pulsar/happy-path-cw20");
    let mut app = TestApp::new(path);
    let genesis = cw20_genesis(&sender);
    app.init(&genesis, "cw-chain");

    let msg = WasmMsg::StoreCode {
        sender: sender.clone(),
        code: CW20_BASE.into(),
    };
    let tx = TxBuilder::new()
        .with_msg(msg)
        .with_signer(&signer, 0)
        .with_fee(1_000_000, coin(50_000, "upulsar"));
    let res = app.block(&[tx]);
    assert_block_success(&res, 1);

    // parse code_id from first message of first tx
    let events = msg_events(&res[0], 0);
    let code_id = event_value(events, "store_code", "code_id")
        .unwrap()
        .parse()
        .unwrap();

    let msg = cw20_base::msg::InstantiateMsg {
        name: "pulsar".to_string(),
        symbol: "PLS".to_string(),
        decimals: 6,
        initial_balances: vec![Cw20Coin {
            address: sender.to_string(),
            amount: Uint128::new(50_000_000),
        }],
        mint: None,
        marketing: None,
    };
    let cw20_msg = to_vec(&msg).unwrap();
    let msg = WasmMsg::Instantiate {
        sender: sender.clone(),
        admin: None,
        code_id,
        msg: cw20_msg.into(),
        funds: vec![],
        label: "My first token".into(),
    };
    let tx = TxBuilder::new()
        .with_msg(msg)
        .with_signer(&signer, 1)
        .with_fee(100_000, coin(5_000, "upulsar"));

    let res = app.block(&[tx]);
    assert_block_success(&res, 1);

    // parse address from first message of first tx
    let events = msg_events(&res[0], 0);
    let contract =
        AccountId::parse_string(event_value(events, "instantiate", "_contract_address").unwrap())
            .unwrap();

    // query balance
    let my_bal = query_cw20_balance(&app, &contract, &sender);
    assert_eq!(my_bal.u128(), 50_000_000);
    let your_bal = query_cw20_balance(&app, &contract, &rcpt);
    assert_eq!(your_bal.u128(), 0);

    // send token
    let tx = TxBuilder::new()
        .with_msg(WasmMsg::Execute {
            sender: sender.clone(),
            contract_addr: contract.clone(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Transfer {
                recipient: rcpt.to_string(),
                amount: Uint128::new(10_000_000),
            })
            .unwrap(),
            funds: vec![],
        })
        .with_signer(&signer, 2);
    let res = app.block(&[tx]);
    assert_block_success(&res, 1);

    // query balance
    let my_bal = query_cw20_balance(&app, &contract, &sender);
    assert_eq!(my_bal.u128(), 40_000_000);
    let your_bal = query_cw20_balance(&app, &contract, &rcpt);
    assert_eq!(your_bal.u128(), 10_000_000);
}
