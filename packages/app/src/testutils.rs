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
        let mut app = App::new(storage, logic);

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

pub struct TxBuilder {
    msgs: Vec<Msg>,
    fee: FeeInfo,
    sender: Option<AccountId>,
    signer: Option<(PrivateKey, u64)>,
    invalid_sig: bool,
}

impl TxBuilder {
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

    pub fn with_fee_info(mut self, fee_info: FeeInfo) -> Self {
        self.fee = fee_info;
        self
    }

    // use to simulate unsigned tx, or tx signed by other account
    // if you set a signer, we will determine this automatically
    pub fn with_sender(mut self, sender: AccountId) -> Self {
        self.sender = Some(sender);
        self
    }

    // use to simulate unsigned tx, or tx signed by other account
    // if you set a signer, we will determine this automatically
    pub fn with_signer(mut self, signer: PrivateKey, sequence: u64) -> Self {
        self.signer = Some((signer, sequence));
        self
    }

    pub fn build(&self) -> Tx {
        // hardcode this for now..
        let message_hash = Binary::from(
            hex!("00cafe00deadbeef00cafe009e0b70086ba7c40e91a9d04d31946c10346d79a9").as_slice(),
        );

        let (sequence, pubkey, mut signature) = match self.signer.as_ref() {
            Some((signer, sequence)) => (
                *sequence,
                Some(signer.to_pubkey()),
                signer.sign(&message_hash),
            ),
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
            fee: FeeInfo {
                fee: None,
                gas_limit: 0,
            },
            timeout_height: None,
            raw_tx: Bytes::from("Hardedcoded value for now"),
        })
    }
}

use cosmrs::crypto::secp256k1;

pub struct PrivateKey(secp256k1::SigningKey);

impl PrivateKey {
    pub fn to_pubkey(&self) -> PubKey {
        let pk = self.0.public_key();
        match pk.type_url() {
            cosmrs::crypto::PublicKey::ED25519_TYPE_URL => PubKey::ed25519(pk.to_bytes()),
            cosmrs::crypto::PublicKey::SECP256K1_TYPE_URL => PubKey::secp256k1(pk.to_bytes()),
            url => panic!("UnsupportedPubKey {url}"),
        }
    }

    pub fn sign(&self, message_hash: &[u8]) -> Binary {
        let signature = self.0.sign(message_hash).unwrap();
        signature.to_vec().into()
    }
}

#[cfg(test)]
mod test {
    use cosmwasm_std::coin;

    use crate::genesis::{BankAccount, WasmParams};

    use super::*;

    #[test]
    fn can_init_and_query_chain() {
        // FIXME: simplify genesis building?
        let account = AccountId::unchecked("foobar");
        let balance = vec![coin(1_000_000, "upulsar"), coin(2_000_000, "umagic")];
        let genesis = GenesisState {
            bank: vec![BankAccount {
                address: account.to_string(),
                balance,
            }],
            wasm: WasmParams {
                gov_account: account.to_string(),
            },
        };

        let mut app = TestApp::new("can_init_and_query_chain");
        app.init(&genesis, "super-chain");

        let bal = app.balance(&account, "upulsar").unwrap();
        assert_eq!(bal.u128(), 1_000_000);
    }

    #[test]
    fn can_check_tx() {}
}
