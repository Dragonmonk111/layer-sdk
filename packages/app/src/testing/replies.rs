use cosmwasm_std::{coin, coins, to_binary, Event};
use pulsar_std::api::TxResponse;
use pulsar_std::{AccountId, MsgData, WasmMsg, WasmMsgData};

use crate::genesis::{BankAccount, GenesisState, WasmParams};
use crate::testing::utils::*;

const ECHO: &[u8] = include_bytes!("../../fixtures/tc_echo.wasm");
const CALLER: &[u8] = include_bytes!("../../fixtures/tc_caller.wasm");
const DENOM: &str = "urepl";

fn genesis(account: &AccountId) -> GenesisState {
    GenesisState {
        bank: vec![BankAccount {
            address: account.to_string(),
            balance: coins(100_000_000, DENOM),
        }],
        wasm: WasmParams {
            gov_account: account.to_string(),
        },
    }
}

struct SetupData {
    app: TestApp,
    signer: PrivateKey,
    echo_id: u64,
    caller_id: u64,
}

// create and init app
// store the hackatom code
fn setup(path: &str) -> SetupData {
    let signer = PrivateKey::random();
    let sender = signer.account_id();

    let path = prepare_cache(path);
    let mut app = TestApp::new(path);
    let genesis = genesis(&sender);
    app.init(&genesis, "replies-1");

    let msg = WasmMsg::StoreCode {
        sender: sender.clone(),
        code: ECHO.into(),
    };
    let tx = TxBuilder::new()
        .with_msg(msg)
        .with_signer(&signer, 0)
        .with_fee(2_000_000, coin(50_000, DENOM));
    let mut res = app.block(&[tx]);
    assert_block_success(&res, 1);
    // parse code_id from response data
    let MsgData::Wasm(WasmMsgData::Store {
        code_id: echo_id, ..
    }) = res.remove(0).result.unwrap().data.remove(0) else {
        panic!("unexpected response data");
    };

    let msg = WasmMsg::StoreCode {
        sender,
        code: CALLER.into(),
    };
    let tx = TxBuilder::new()
        .with_msg(msg)
        .with_signer(&signer, 1)
        .with_fee(2_000_000, coin(50_000, DENOM));
    let mut res = app.block(&[tx]);
    assert_block_success(&res, 1);
    // parse code_id from response data
    let MsgData::Wasm(WasmMsgData::Store {
        code_id: caller_id, ..
    }) = res.remove(0).result.unwrap().data.remove(0) else {
        panic!("unexpected response data");
    };

    SetupData {
        app,
        signer,
        echo_id,
        caller_id,
    }
}

// This will instantiate a new hackatom instance from the given code and given initial balance,
// and return the contract address. Will panic on error
#[track_caller]
fn init_contract(
    app: &mut TestApp,
    signer: &PrivateKey,
    caller_id: u64,
    init_msg: &tc_caller::InstantiateMsg,
) -> (AccountId, TxResponse) {
    // find the proper sequence
    let sender = signer.account_id();
    let sequence = app.sequence(&sender).unwrap();

    let msg = WasmMsg::Instantiate {
        sender: sender.clone(),
        admin: Some(sender),
        code_id: caller_id,
        msg: to_binary(init_msg).unwrap(),
        funds: vec![],
        label: "Caller Contract".into(),
    };
    let tx = TxBuilder::new().with_msg(msg).with_signer(signer, sequence);
    let mut res = app.block(&[tx]);
    assert_block_success(&res, 1);
    let res = res.remove(0);

    // parse address from first message of first tx
    let events = msg_events(&res, 0);
    let contract =
        AccountId::parse_string(event_value(events, "instantiate", "_contract_address").unwrap())
            .unwrap();

    (contract, res.result.unwrap())
}

#[test]
fn basic_init_and_execute() {
    let SetupData {
        mut app,
        signer,
        caller_id,
        echo_id,
    } = setup("/tmp/pulsar/basic_init_and_execute");
    let sender = signer.account_id();

    let subcall = tc_caller::CallInfo {
        reply_on: cosmwasm_std::ReplyOn::Always,
        override_data: false,
        gas_limit: None,
    };

    // simple init message with no problems
    let init_msg = tc_caller::InstantiateMsg {
        code_id: echo_id,
        msg: tc_echo::InstantiateMsg::Echo(tc_echo::EchoMsg {
            data: Some(b"from echo".into()),
            attrs: vec![],
            events: vec![Event::new("test-one").add_attribute("some", "more")],
        }),
        subcall: subcall.clone(),
    };

    // create contract instance with 10_000_000 tokens
    let (contract, _res) = init_contract(&mut app, &signer, caller_id, &init_msg);

    // TODO: lots of verification

    // catch a failure from the contract
    let exec_msg = tc_caller::ExecuteMsg {
        msg: tc_echo::ExecuteMsg::Fail {
            msg: "Oh, no!".to_string(),
        },
        subcall,
    };
    let sequence = app.sequence(&sender).unwrap();
    let tx = TxBuilder::new()
        .with_msg(WasmMsg::Execute {
            sender,
            contract_addr: contract,
            msg: to_binary(&exec_msg).unwrap(),
            funds: vec![],
        })
        .with_signer(&signer, sequence);
    let mut res = app.block(&[tx]);
    assert_block_success(&res, 1);
    let _res = res.remove(0);

    // TODO: verify the data here

    // TODO: query the counts on caller
    // TODO: query the echo contract address
    // TODO: query the counts on echo
}
