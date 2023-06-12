#![cfg(test)]

use bytes::Bytes;
use cosmwasm_std::{testing::mock_env, to_binary, Binary, Coin, Uint128};
use hex_literal::hex;
use pulsar_std::{
    api::{Block, InitChainRequest, TmPubKey, TxResult, ValidatorUpdate},
    response::{BankQueryResponse, QueryResponse},
    AccountId, BankQuery, FeeInfo, Msg, PubKey, Query, SignedTx, SigningInfo, Tx,
};
use pulsar_storage::MemoryStore;

use crate::{app::App, genesis::GenesisState, AppConfig, PulsarError, PulsarResult, StateMachine};

const NANO_SECOND_PER_BLOCK: u64 = 2_400 * 1_000_000; // 2.4 seconds

pub struct TestApp {
    pub app: App<MemoryStore>,
}

impl TestApp {
    pub fn new(wasm_dir: &str) -> Self {
        let storage = MemoryStore::default();
        let logic = StateMachine::new(&AppConfig::new(wasm_dir));

        // create the app
        let app = App::new(storage, logic);

        Self { app }
    }

    // TODO: use builder pattern for config
    pub fn init(&mut self, genesis: &GenesisState, chain_id: &str) {
        let app_state = to_binary(genesis).unwrap();
        let env = mock_env();
        let request = InitChainRequest {
            time: env.block.time,
            chain_id: chain_id.to_string(),
            consensus_params: Default::default(),
            validators: vec![ValidatorUpdate {
                pub_key: TmPubKey::Ed25519(vec![123u8; 32]),
                power: 1_000_000,
            }],
            app_state,
            initial_height: 1,
        };
        self.app.init(request).unwrap();
        // process empty genesis block
        self.block(&[]);
    }

    pub fn height(&self) -> u64 {
        self.app.info().unwrap().height
    }

    pub fn check_tx(&self, tx: &TxBuilder) -> TxResult<PulsarError> {
        self.app.check_tx(tx.build())
    }

    pub fn block(&mut self, txs: &[TxBuilder]) -> Vec<TxResult<PulsarError>> {
        let info = self.app.info().unwrap();

        let block = Block {
            txs: txs.iter().map(|tx| tx.build()).collect(),
            height: info.height + 1,
            time: info.time.plus_nanos(NANO_SECOND_PER_BLOCK),
            proposer_address: vec![1u8; 32],
            last_votes: vec![],
        };
        self.app.finalize_block(block).unwrap().tx_results
    }

    pub fn query(&self, query: impl Into<Query>) -> PulsarResult<QueryResponse<PulsarError>> {
        self.app.query(query.into())
    }

    pub fn simulate(&self, tx: &TxBuilder) -> PulsarResult<TxResult<PulsarError>> {
        let res = self.query(Query::Simulate(tx.build()))?;
        match res {
            QueryResponse::Simulate(res) => Ok(res),
            _ => panic!("unexpected response"),
        }
    }

    pub fn all_balances(&self, account: &AccountId) -> PulsarResult<Vec<Coin>> {
        let res = self.query(BankQuery::AllBalances {
            address: account.clone(),
        })?;
        match res {
            QueryResponse::Bank(BankQueryResponse::AllBalances(bal)) => Ok(bal.amount),
            _ => panic!("unexpected response"),
        }
    }

    pub fn balance(&self, account: &AccountId, denom: &str) -> PulsarResult<Uint128> {
        let res = self.query(BankQuery::Balance {
            address: account.clone(),
            denom: denom.to_string(),
        })?;
        match res {
            QueryResponse::Bank(BankQueryResponse::Balance(bal)) => Ok(bal.amount.amount),
            _ => panic!("unexpected response"),
        }
    }
}

pub struct TxBuilder<'a> {
    msgs: Vec<Msg>,
    fee: FeeInfo,
    sender: Option<AccountId>,
    signer: Option<(&'a PrivateKey, u64)>,
    invalid_sig: bool,
}

impl<'a> TxBuilder<'a> {
    pub fn new() -> Self {
        Self {
            msgs: vec![],
            fee: FeeInfo::default(),
            sender: None,
            signer: None,
            invalid_sig: false,
        }
    }

    pub fn with_msg(mut self, msg: impl Into<Msg>) -> Self {
        self.msgs.push(msg.into());
        self
    }

    pub fn with_fee_info(mut self, gas_limit: u64, fee: Coin) -> Self {
        self.fee = FeeInfo {
            gas_limit,
            fee: Some(fee),
        };
        self
    }

    // use to simulate unsigned tx, or tx signed by other account
    // if you set a signer, we will determine this automatically
    pub fn with_sender(mut self, sender: &AccountId) -> Self {
        self.sender = Some(sender.clone());
        self
    }

    // use to simulate unsigned tx, or tx signed by other account
    // if you set a signer, we will determine this automatically
    pub fn with_signer(mut self, signer: &'a PrivateKey, sequence: u64) -> Self {
        self.signer = Some((signer, sequence));
        self
    }

    pub fn with_invalid_sig(mut self) -> Self {
        self.invalid_sig = true;
        self
    }

    pub fn build(&self) -> Tx {
        // hardcode this for now..
        let raw_tx = Vec::from(
            hex!("00cafe00deadbeef00cafe009e0b70086ba7c40e91a9d04d31946c10346d79a9").as_slice(),
        );
        let message_hash = Sha256::new_with_prefix(&raw_tx).finalize().to_vec().into();

        let (sequence, pubkey, mut signature) = match self.signer.as_ref() {
            Some((signer, sequence)) => (*sequence, Some(signer.to_pubkey()), signer.sign(&raw_tx)),
            None => (0, None, Binary::from(b"")),
        };
        if self.invalid_sig && !signature.is_empty() {
            // this is a hack to make the signature invalid
            // we just flip the first bit
            signature.0[0] ^= 0x80;
        }

        let sender = self
            .sender
            .clone()
            .unwrap_or_else(|| pubkey.as_ref().unwrap().account_id().unwrap());

        Tx::Signed(SignedTx {
            msgs: self.msgs.clone(),
            signer: sender,
            signing_info: SigningInfo {
                message_hash,
                sequence,
                pubkey,
                signature,
            },
            fee: self.fee.clone(),
            timeout_height: None,
            raw_tx: Bytes::from(raw_tx),
        })
    }
}

use k256::ecdsa::{signature::DigestSigner, Signature, SigningKey};
use k256::elliptic_curve::rand_core::OsRng;
use sha2::{Digest, Sha256};

pub struct PrivateKey(SigningKey);

impl PrivateKey {
    #[allow(dead_code)]
    pub fn from_slice(secret: &[u8]) -> Self {
        let sk = SigningKey::from_slice(secret).unwrap();
        Self(sk)
    }

    pub fn random() -> Self {
        let sk = SigningKey::random(&mut OsRng);
        Self(sk)
    }

    pub fn to_pubkey(&self) -> PubKey {
        let pk = self.0.verifying_key().to_encoded_point(true);
        PubKey::secp256k1(Binary::from(pk.as_bytes()))
    }

    pub fn sign(&self, message: &[u8]) -> Binary {
        let digest = Sha256::new_with_prefix(message);
        let signature: Signature = self.0.sign_digest(digest);
        signature.to_vec().into()
    }
}

#[cfg(test)]
mod test {
    use cosmwasm_std::coin;
    use pulsar_std::BankMsg;

    use crate::genesis::{BankAccount, WasmParams};

    use super::*;

    // FIXME: use genesis building pattern?
    fn sample_genesis(account: &AccountId) -> GenesisState {
        let balance = vec![coin(1_000_000, "upulsar"), coin(2_000_000, "umagic")];
        GenesisState {
            bank: vec![BankAccount {
                address: account.to_string(),
                balance,
            }],
            wasm: WasmParams {
                gov_account: account.to_string(),
            },
        }
    }

    #[test]
    fn can_init_and_query_chain() {
        let account = AccountId::unchecked("foobar");
        let genesis = sample_genesis(&account);

        let mut app = TestApp::new("can_init_and_query_chain");
        app.init(&genesis, "super-chain");

        let bal = app.balance(&account, "upulsar").unwrap();
        assert_eq!(bal.u128(), 1_000_000);

        let bals = app.all_balances(&account).unwrap();
        let expected = vec![coin(2_000_000, "umagic"), coin(1_000_000, "upulsar")];
        assert_eq!(bals, expected);
    }

    #[test]
    fn run_empty_blocks() {
        let account = AccountId::unchecked("foobar");
        let genesis = sample_genesis(&account);

        let mut app = TestApp::new("run_empty_blocks");
        app.init(&genesis, "super-chain");

        app.block(&[]);
        app.block(&[]);
        app.block(&[]);

        // query works
        let bal = app.balance(&account, "upulsar").unwrap();
        assert_eq!(bal.u128(), 1_000_000);

        // height is 4
        assert_eq!(app.height(), 4);
    }

    #[test]
    fn can_check_tx() {
        let pk = PrivateKey::random();
        let signer = pk.to_pubkey();
        let acct = signer.account_id().unwrap();
        let rcpt = AccountId::unchecked("getting paid");

        let mut app = TestApp::new("can_check_tx");
        let genesis = sample_genesis(&acct);
        app.init(&genesis, "super-chain");

        // build and check tx
        let tx = TxBuilder::new()
            .with_msg(BankMsg::Send {
                sender: acct.clone(),
                recipient: rcpt.clone(),
                amount: vec![coin(123_000, "upulsar")],
            })
            .with_signer(&pk, 0);

        // make sure it works
        app.check_tx(&tx).result.unwrap();

        // and bad tx works, as long as we pay fees
        let tx = TxBuilder::new()
            .with_msg(BankMsg::Send {
                sender: rcpt.clone(),
                recipient: acct.clone(),
                amount: vec![coin(123_000, "upulsar")],
            })
            .with_fee_info(100_000, coin(300_000, "upulsar"))
            .with_signer(&pk, 0);

        // make sure it succeeds
        app.check_tx(&tx).result.unwrap();
    }

    #[test]
    fn check_tx_failures() {
        let pk = PrivateKey::random();
        let signer = pk.to_pubkey();
        let acct = signer.account_id().unwrap();
        let rcpt = AccountId::unchecked("getting paid");

        let mut app = TestApp::new("can_check_tx");
        let genesis = sample_genesis(&acct);
        app.init(&genesis, "super-chain");

        // but too many fees fails
        let tx = TxBuilder::new()
            .with_msg(BankMsg::Send {
                sender: acct.clone(),
                recipient: rcpt.clone(),
                amount: vec![coin(123_000, "upulsar")],
            })
            .with_fee_info(100_000, coin(3_000_000, "upulsar"))
            .with_signer(&pk, 0);
        app.check_tx(&tx).result.unwrap_err();

        // as does an invalid signature
        let tx = TxBuilder::new()
            .with_msg(BankMsg::Send {
                sender: acct.clone(),
                recipient: rcpt.clone(),
                amount: vec![coin(123_000, "upulsar")],
            })
            .with_invalid_sig()
            .with_signer(&pk, 0);
        app.check_tx(&tx).result.unwrap_err();
    }

    #[test]
    fn can_simulate_tx() {
        let sender = AccountId::unchecked("no private key");
        let rcpt = AccountId::unchecked("getting paid");

        let mut app = TestApp::new("can_check_tx");
        let genesis = sample_genesis(&sender);
        app.init(&genesis, "super-chain");

        // build and simulate tx (even without private key)
        let tx = TxBuilder::new()
            .with_msg(BankMsg::Send {
                sender: sender.clone(),
                recipient: rcpt.clone(),
                amount: vec![coin(123_000, "upulsar")],
            })
            .with_sender(&sender);

        // make sure it works (even without signer)
        app.simulate(&tx).unwrap();
    }
}
