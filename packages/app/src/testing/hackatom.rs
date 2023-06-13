use cosmwasm_std::{coin, coins, to_binary};
use pulsar_std::{AccountId, WasmMsg};

use crate::genesis::{BankAccount, GenesisState, WasmParams};
use crate::testing::utils::*;

// v1.2.6
const HACKATOM: &[u8] = include_bytes!("../../fixtures/hackatom.wasm");
const DENOM: &str = "uhack";

fn hackatom_genesis(account: &AccountId) -> GenesisState {
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

// create and init app
// store the hackatom code
fn setup(path: &str) -> (TestApp, PrivateKey, u64) {
    let signer = PrivateKey::random();
    let sender = signer.account_id();

    let path = prepare_cache(path);
    let mut app = TestApp::new(path);
    let genesis = hackatom_genesis(&sender);
    app.init(&genesis, "hackatom");

    let msg = WasmMsg::StoreCode {
        sender,
        code: HACKATOM.into(),
    };
    let tx = TxBuilder::new()
        .with_msg(msg)
        .with_signer(&signer, 0)
        .with_fee(1_000_000, coin(50_000, DENOM));
    let res = app.block(&[tx]);
    assert_block_success(&res, 1);

    // parse code_id from first message of first tx
    let events = msg_events(&res[0], 0);
    let code_id = event_value(events, "store_code", "code_id")
        .unwrap()
        .parse()
        .unwrap();

    (app, signer, code_id)
}

// This will instantiate a new hackatom instance from the given code and given initial balance,
// and return the contract address. Will panic on error
#[track_caller]
fn init_contract(
    app: &mut TestApp,
    code_id: u64,
    signer: &PrivateKey,
    verifier: &AccountId,
    beneficiary: &AccountId,
    funds: u128,
) -> AccountId {
    // find the proper sequence
    let sender = signer.account_id();
    let sequence = app.sequence(&sender).unwrap();

    let init_msg = msgs::InstantiateMsg {
        verifier: verifier.to_string(),
        beneficiary: beneficiary.to_string(),
    };
    let msg = WasmMsg::Instantiate {
        sender: sender.clone(),
        admin: Some(sender),
        code_id,
        msg: to_binary(&init_msg).unwrap(),
        funds: coins(funds, DENOM),
        label: "Hackatom Contract".into(),
    };
    let tx = TxBuilder::new().with_msg(msg).with_signer(signer, sequence);
    let res = app.block(&[tx]);
    assert_block_success(&res, 1);
    // parse address from first message of first tx
    let events = msg_events(&res[0], 0);
    let contract =
        AccountId::parse_string(event_value(events, "instantiate", "_contract_address").unwrap())
            .unwrap();
    contract
}

#[test]
fn basic_hackatom_usage() {
    let (mut app, signer, code_id) = setup("/tmp/pulsar/basic-hackatom-usage");

    // other actors
    let verify_key = PrivateKey::random();
    let verifier = verify_key.to_pubkey().account_id().unwrap();
    let beneficiary = AccountId::unchecked("beneficiary");

    // create contract instance with 10_000_000 tokens
    let contract = init_contract(
        &mut app,
        code_id,
        &signer,
        &verifier,
        &beneficiary,
        10_000_000,
    );

    // verify it is set up properly
    let hacks = app.balance(&contract, DENOM).unwrap();
    assert_eq!(hacks.u128(), 10_000_000);
    let r: msgs::VerifierResponse = app
        .query_wasm(&contract, &msgs::QueryMsg::Verifier {})
        .unwrap();
    assert_eq!(r.verifier, verifier.to_string());

    // verifier can release tokens to the beneficiary
    // TODO
}

#[test]
fn check_message_loop() {
    // install send funds, release funds
}

#[test]
fn check_cpu_loop() {
    let (mut app, signer, code_id) = setup("/tmp/pulsar/check-cpu-loop");

    // other actors
    let sender = signer.account_id();
    let verify_key = PrivateKey::random();
    let verifier = verify_key.account_id();
    let beneficiary = AccountId::unchecked("beneficiary");

    // create contract instance with 10_000_000 tokens
    let contract = init_contract(
        &mut app,
        code_id,
        &signer,
        &verifier,
        &beneficiary,
        10_000_000,
    );

    let gas_limit = 831_000;
    let tx = TxBuilder::new()
        .with_msg(WasmMsg::Execute {
            sender,
            contract_addr: contract,
            msg: to_binary(&msgs::ExecuteMsg::CpuLoop {}).unwrap(),
            funds: vec![],
        })
        .with_fee(gas_limit, coin(1_000, DENOM))
        .with_signer(&signer, 2);
    let res = app.block(&[tx]);
    assert_eq!(res.len(), 1);
    assert!(res[0].result.is_err());
    assert!(res[0].gas.gas_used >= gas_limit);
    assert!(res[0].gas.gas_used < gas_limit + 10_000);
}

// Note: storage is priced about 5x cheaper than it should compared to cpu usage
// (Free to read from cache is not correctly priced)
#[test]
fn check_storage_loop() {
    let (mut app, signer, code_id) = setup("/tmp/pulsar/check-storage-loop");

    // other actors
    let sender = signer.account_id();
    let verify_key = PrivateKey::random();
    let verifier = verify_key.account_id();
    let beneficiary = AccountId::unchecked("beneficiary");

    // create contract instance with 10_000_000 tokens
    let contract = init_contract(
        &mut app,
        code_id,
        &signer,
        &verifier,
        &beneficiary,
        10_000_000,
    );

    // execute storage loop with gas limit
    let gas_limit = 376_000;
    let tx = TxBuilder::new()
        .with_msg(WasmMsg::Execute {
            sender,
            contract_addr: contract,
            msg: to_binary(&msgs::ExecuteMsg::StorageLoop {}).unwrap(),
            funds: vec![],
        })
        .with_fee(gas_limit, coin(1_000, DENOM))
        .with_signer(&signer, 2);
    let res = app.block(&[tx]);
    assert_eq!(res.len(), 1);
    assert!(res[0].result.is_err());
    assert!(res[0].gas.gas_used >= gas_limit);
    assert!(res[0].gas.gas_used < gas_limit + 10_000);
}

#[test]
fn check_query_recursion() {
    let (mut app, signer, code_id) = setup("/tmp/pulsar/check-query-recursion");

    // other actors
    let verify_key = PrivateKey::random();
    let verifier = verify_key.account_id();
    let beneficiary = AccountId::unchecked("beneficiary");

    // create contract instance with 10_000_000 tokens
    let contract = init_contract(
        &mut app,
        code_id,
        &signer,
        &verifier,
        &beneficiary,
        10_000_000,
    );

    // use internal dispatched query and compare to normal query
    let query = &msgs::QueryMsg::Recurse {
        depth: 10,
        work: 100,
    };
    let rec: msgs::RecurseResponse = app.query_wasm(&contract, &query).unwrap();
    assert_eq!(rec.hashed.len(), 32);
}

#[test]
fn check_query_balance() {
    let (mut app, signer, code_id) = setup("/tmp/pulsar/check-query-balance");

    // other actors
    let sender = signer.account_id();
    let verify_key = PrivateKey::random();
    let verifier = verify_key.account_id();
    let beneficiary = AccountId::unchecked("beneficiary");

    // create contract instance with 10_000_000 tokens
    let contract = init_contract(
        &mut app,
        code_id,
        &signer,
        &verifier,
        &beneficiary,
        10_000_000,
    );

    // use internal dispatched query and compare to normal query
    let query = &msgs::QueryMsg::OtherBalance {
        address: contract.to_string(),
    };
    let my_bal: cosmwasm_std::AllBalanceResponse = app.query_wasm(&contract, &query).unwrap();
    let expected = app.all_balances(&contract).unwrap();
    assert_eq!(my_bal.amount, expected);

    // use internal dispatched query
    let query = &msgs::QueryMsg::OtherBalance {
        address: sender.to_string(),
    };
    let sender_bal: cosmwasm_std::AllBalanceResponse = app.query_wasm(&contract, &query).unwrap();
    let expected = app.all_balances(&sender).unwrap();
    assert_eq!(sender_bal.amount, expected)
}

/// This is copied from https://github.com/CosmWasm/cosmwasm/blob/v1.2.6/contracts/hackatom/src/msg.rs
pub mod msgs {
    use cosmwasm_schema::{cw_serde, QueryResponses};

    use cosmwasm_std::{Binary, Coin};

    #[cw_serde]
    pub struct InstantiateMsg {
        pub verifier: String,
        pub beneficiary: String,
    }

    /// MigrateMsg allows a privileged contract administrator to run
    /// a migration on the contract. In this (demo) case it is just migrating
    /// from one hackatom code to the same code, but taking advantage of the
    /// migration step to set a new validator.
    ///
    /// Note that the contract doesn't enforce permissions here, this is done
    /// by blockchain logic (in the future by blockchain governance)
    #[cw_serde]
    pub struct MigrateMsg {
        pub verifier: String,
    }

    /// SudoMsg is only exposed for internal Cosmos SDK modules to call.
    /// This is showing how we can expose "admin" functionality than can not be called by
    /// external users or contracts, but only trusted (native/Go) code in the blockchain
    #[cw_serde]
    pub enum SudoMsg {
        StealFunds {
            recipient: String,
            amount: Vec<Coin>,
        },
    }

    // failure modes to help test wasmd, based on this comment
    // https://github.com/cosmwasm/wasmd/issues/8#issuecomment-576146751
    #[cw_serde]
    pub enum ExecuteMsg {
        /// Releasing all funds in the contract to the beneficiary. This is the only "proper" action of this demo contract.
        Release {},
        /// Infinite loop to burn cpu cycles (only run when metering is enabled)
        CpuLoop {},
        /// Infinite loop making storage calls (to test when their limit hits)
        StorageLoop {},
        /// Infinite loop reading and writing memory
        MemoryLoop {},
        /// Infinite loop sending message to itself
        MessageLoop {},
        /// Allocate large amounts of memory without consuming much gas
        AllocateLargeMemory { pages: u32 },
        /// Trigger a panic to ensure framework handles gracefully
        Panic {},
        /// Starting with CosmWasm 0.10, some API calls return user errors back to the contract.
        /// This triggers such user errors, ensuring the transaction does not fail in the backend.
        UserErrorsInApiCalls {},
    }

    #[cw_serde]
    #[derive(QueryResponses)]
    pub enum QueryMsg {
        /// returns a human-readable representation of the verifier
        /// use to ensure query path works in integration tests
        #[returns(VerifierResponse)]
        Verifier {},
        /// This returns cosmwasm_std::AllBalanceResponse to demo use of the querier
        #[returns(cosmwasm_std::AllBalanceResponse)]
        OtherBalance { address: String },
        /// Recurse will execute a query into itself up to depth-times and return
        /// Each step of the recursion may perform some extra work to test gas metering
        /// (`work` rounds of sha256 on contract).
        /// Now that we have Env, we can auto-calculate the address to recurse into
        #[returns(RecurseResponse)]
        Recurse { depth: u32, work: u32 },
        /// GetInt returns a hardcoded u32 value
        #[returns(IntResponse)]
        GetInt {},
    }

    #[cw_serde]
    pub struct VerifierResponse {
        pub verifier: String,
    }

    #[cw_serde]
    pub struct RecurseResponse {
        /// hashed is the result of running sha256 "work+1" times on the contract's human address
        pub hashed: Binary,
    }

    #[cw_serde]
    pub struct IntResponse {
        pub int: u32,
    }
}
