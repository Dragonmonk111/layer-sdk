use serde::Serialize;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::fmt::Debug;
use std::rc::Rc;

use abstract_cw_multi_test::AppResponse;
use cosmwasm_std::{to_json_binary, Addr, Binary, BlockInfo, Coin, StdError};

use cw_orch_core::contract::{interface_traits::Uploadable, WasmPath};
use cw_orch_core::environment::{ChainInfo, ChainKind, ChainState, NetworkInfo, TxHandler};

use slay3r_app::{App, AppConfig, PulsarError, StateMachine};
use slay3r_std::api::{Block, TxResult};
use slay3r_std::{AccountId, FeeInfo, Msg, SigningInfo, WasmMsg};
use slay3r_storage::MemoryStore;

use crate::OrchRegistry;

/// Mock Chain info for osmosis test tube. This is used to get the right wasm
pub const MOCK_CHAIN_INFO: ChainInfo = ChainInfo {
    chain_id: "slay3r-orch",
    gas_denom: "uslay",
    gas_price: 0.025,
    grpc_urls: &[],
    lcd_url: None,
    fcd_url: None,
    network_info: NetworkInfo {
        chain_name: "slay3r",
        pub_address_prefix: "slay3r",
        coin_type: 118u32,
    },
    kind: ChainKind::Local,
};

#[derive(Clone)]
pub struct Slay3rTube {
    pub config: TubeConfig,
    /// Address used for the operations.
    // pub sender: Rc<SigningAccount>,
    /// Inner mutable state storage for contract addresses and code-ids
    pub state: Rc<RefCell<OrchRegistry>>,
    // Inner mutable app backend
    pub app: Rc<RefCell<App<MemoryStore>>>,
}

#[derive(Debug, Clone)]
pub struct TubeConfig {
    /// Block time in milliseconds.
    pub block_time_ms: u64,
    /// Default gas price
    pub gas_price: f64,
    pub gas_denom: String,
    // Chain ID
    pub chain_id: String,
}

impl Default for TubeConfig {
    fn default() -> Self {
        // FIXME: can we configure this along with the MOCK_CHAIN_INFO somehow?
        Self {
            block_time_ms: 2000, // used to update artifical clock when we move forward a block (1 block = 2 second)
            gas_price: MOCK_CHAIN_INFO.gas_price,
            gas_denom: MOCK_CHAIN_INFO.gas_denom.to_string(),
            chain_id: MOCK_CHAIN_INFO.chain_id.to_string(),
        }
    }
}

fn wrap<T>(val: T) -> Rc<RefCell<T>> {
    Rc::new(RefCell::new(val))
}

impl Slay3rTube {
    pub fn new(cache_dir: &str) -> Self {
        let sm = StateMachine::new(&AppConfig::new(cache_dir));
        let app = App::new(MemoryStore::new(), sm);
        let config = TubeConfig::default();
        Self {
            // FIXME: make chain_id configurable?
            state: wrap(OrchRegistry::new(MOCK_CHAIN_INFO.chain_id)),
            app: wrap(app),
            config,
        }
    }

    // TODO: init
    // {
    //     let genesis = GenesisState {
    //         bank: vec![BankAccount {
    //             address: sender.to_string(),
    //             balance: coins(2_000_000_000, denom),
    //         }],
    //         wasm: WasmParams {
    //             gov_account: sender.to_string(),
    //         },
    //     };
    //     // TODO: remove from App args, build inside (with config)
    //     let logic = StateMachine::new(&AppConfig::new("/tmp/slay3r/transaction_workflow"));
    //     let request = mock_init(&genesis);

    //     // create the app
    //     let mut app = App::new(storage, logic);
    //     app.init(request).unwrap();

    // }

    pub fn run_block(
        &self,
        tx: Vec<slay3r_std::Tx>,
    ) -> Result<Vec<TxResult<PulsarError>>, PulsarError> {
        let mut app = self.app.borrow_mut();
        let info = app.info().unwrap();
        // TODO: non-empty validators
        let full_block = Block {
            txs: tx,
            height: info.height + 1,
            time: info.time.plus_nanos(self.config.block_time_ms * 1_000_000), // in ms
            proposer_address: vec![],
            last_votes: vec![],
        };
        let res = app.finalize_block(full_block)?;
        Ok(res.tx_results)
    }

    pub fn block_info(&self) -> BlockInfo {
        let app = self.app.borrow();
        app.info().unwrap().clone()
    }

    // simple helper for the usual one msg/one tx case
    pub(crate) fn prepare_tx(&self, msg: impl Into<Msg>, signer: AccountId) -> slay3r_std::Tx {
        self.prepare_tx_multi(vec![msg.into()], signer)
    }

    // Use to create a valid "SignedTx" for the given account
    // TODO: implement, needs key material
    pub(crate) fn prepare_tx_multi(&self, msgs: Vec<Msg>, signer: AccountId) -> slay3r_std::Tx {
        // query account info for the sequence

        // generate raw_tx bytes (via Debug?)
        // generate message hash

        // FIXME: simulate gas fees? (right now hardcoded)
        let gas_limit = 50_000_000u64;
        let gas_amount = (gas_limit as f64 * self.config.gas_price) as u128;
        let fee = FeeInfo {
            fee: Some(Coin {
                denom: self.config.gas_denom.clone(),
                amount: gas_amount.into(),
            }),
            gas_limit,
        };

        // placeholder for signing info
        let signing_info = SigningInfo {
            sequence: 0,  // TODO: query this
            pubkey: None, // TODO: get this from signer
            // these intentionally left blank
            signature: Binary::from(b""),
            message_hash: Binary::from(b""),
        };

        // make tx with no real signing info
        let mut tx = slay3r_std::SignedTx {
            msgs,
            signer, // AccountId
            fee,
            timeout_height: None,
            signing_info,
            raw_tx: vec![].into(),
        };

        // generate bytes from debug info, then hash
        let tx_bytes = format!("{:?}", tx).into_bytes();
        let message_hash = Sha256::digest(&tx_bytes).to_vec();
        // TODO: make signature
        let signature = vec![];

        tx.raw_tx = tx_bytes.into();
        tx.signing_info.message_hash = message_hash.into();
        tx.signing_info.signature = signature.into();

        slay3r_std::Tx::Signed(tx)
    }

    // TODO: more init stuff
}

impl ChainState for Slay3rTube {
    type Out = Rc<RefCell<OrchRegistry>>;

    fn state(&self) -> Self::Out {
        self.state.clone()
    }
}

impl TxHandler for Slay3rTube {
    type Response = AppResponse;

    type Error = slay3r_app::PulsarError;

    type ContractSource = WasmPath;

    type Sender = ();

    fn sender(&self) -> Addr {
        todo!()
    }

    fn set_sender(&mut self, _sender: Self::Sender) {
        todo!()
    }

    /// Uploads a contract to the chain.
    fn upload<T: Uploadable>(&self, _contract: &T) -> Result<Self::Response, Self::Error> {
        let sender = AccountId::parse_string(self.sender().as_str())?;
        let signer = sender.clone();

        // load contract wasm
        let file_res = std::fs::read(<T as Uploadable>::wasm(&MOCK_CHAIN_INFO.into()).path());
        let code = file_res
            .map_err(|e| StdError::generic_err(e.to_string()))?
            .into();
        let msg = WasmMsg::StoreCode { sender, code };

        // sign it, run it, convert output
        let tx = self.prepare_tx(msg, signer);
        let res = self.run_block(vec![tx])?.pop().unwrap();
        tx_to_app_response(res)
    }

    /// Send a InstantiateMsg to a contract.
    fn instantiate<I: Serialize + Debug>(
        &self,
        code_id: u64,
        init_msg: &I,
        label: Option<&str>,
        admin: Option<&Addr>,
        coins: &[cosmwasm_std::Coin],
    ) -> Result<Self::Response, Self::Error> {
        // construct message format
        let sender = AccountId::parse_string(self.sender().as_str())?;
        let signer = sender.clone();
        let admin = admin
            .map(|a| AccountId::parse_string(a.as_str()))
            .transpose()?;
        let label = label.unwrap_or("default").to_string();
        let msg = to_json_binary(init_msg)?;
        let msg = WasmMsg::Instantiate {
            sender,
            admin,
            code_id,
            msg,
            funds: coins.into(),
            label,
        };

        // sign it, run it, convert output
        let tx = self.prepare_tx(msg, signer);
        let res = self.run_block(vec![tx])?.pop().unwrap();
        tx_to_app_response(res)
    }

    /// Send a Instantiate2Msg to a contract.
    fn instantiate2<I: Serialize + Debug>(
        &self,
        code_id: u64,
        init_msg: &I,
        label: Option<&str>,
        admin: Option<&Addr>,
        coins: &[cosmwasm_std::Coin],
        salt: Binary,
    ) -> Result<Self::Response, Self::Error> {
        let sender = AccountId::parse_string(self.sender().as_str())?;
        let signer = sender.clone();
        let admin = admin
            .map(|a| AccountId::parse_string(a.as_str()))
            .transpose()?;
        let label = label.unwrap_or("default").to_string();
        let msg = to_json_binary(init_msg)?;
        let msg = WasmMsg::Instantiate2 {
            sender,
            admin,
            code_id,
            msg,
            funds: coins.into(),
            label,
            salt,
        };

        // sign it, run it, convert output
        let tx = self.prepare_tx(msg, signer);
        let res = self.run_block(vec![tx])?.pop().unwrap();
        tx_to_app_response(res)
    }

    /// Send a ExecMsg to a contract.
    fn execute<E: Serialize + Debug>(
        &self,
        exec_msg: &E,
        coins: &[Coin],
        contract_address: &Addr,
    ) -> Result<Self::Response, Self::Error> {
        let sender = AccountId::parse_string(self.sender().as_str())?;
        let signer = sender.clone();
        let contract_addr = AccountId::parse_string(contract_address.as_str())?;
        let msg = to_json_binary(exec_msg)?;
        let msg = WasmMsg::Execute {
            sender,
            contract_addr,
            msg,
            funds: coins.into(),
        };

        // sign it, run it, convert output
        let tx = self.prepare_tx(msg, signer);
        let res = self.run_block(vec![tx])?.pop().unwrap();
        tx_to_app_response(res)
    }

    /// Send a MigrateMsg to a contract.
    fn migrate<M: Serialize + Debug>(
        &self,
        migrate_msg: &M,
        new_code_id: u64,
        contract_address: &Addr,
    ) -> Result<Self::Response, Self::Error> {
        let sender = AccountId::parse_string(self.sender().as_str())?;
        let signer = sender.clone();
        let contract_addr = AccountId::parse_string(contract_address.as_str())?;
        let msg = to_json_binary(migrate_msg)?;
        let msg = WasmMsg::Migrate {
            sender,
            contract_addr,
            new_code_id,
            msg,
        };

        // sign it, run it, convert output
        let tx = self.prepare_tx(msg, signer);
        let res = self.run_block(vec![tx])?.pop().unwrap();
        tx_to_app_response(res)
    }

    /// Clones the chain with a different sender.
    /// Usually used to call a contract as a different sender.
    fn call_as(&self, sender: &<Self as TxHandler>::Sender) -> Self {
        let mut chain = self.clone();
        chain.set_sender(sender.clone());
        chain
    }
}

fn tx_to_app_response(tx: TxResult<PulsarError>) -> Result<AppResponse, PulsarError> {
    // If the tx was a failure, also return error
    let res = tx.result?;
    let events = res.events.into_iter().flatten().collect();
    let data = slay3r_cosmos::msg_data_to_proto(res.data);

    let output = AppResponse {
        data: Some(data.into()),
        events,
    };
    Ok(output)
}
