use cosmwasm_std::{coin, coins, Binary, Coin, Decimal, Uint128, Uint64};
use serde::{Deserialize, Serialize};

pub const DEFAULT_SUPPLY: u128 = 200_000_000_000;
pub const DEFAULT_DENOM: &str = "upulse";

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct BaseCoin {
    pub denom: String,
    pub amount: String,
}

impl Into<Coin> for BaseCoin {
    fn into(self) -> Coin {
        let amount: u128 = parse_int::parse(&self.amount).unwrap();

        Coin {
            denom: self.denom,
            amount: amount.into(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct AuthAccountResponse {
    pub account: AuthAccount,
}

#[derive(Serialize, Deserialize)]
pub struct AuthAccount {
    #[serde(rename = "@type")]
    pub typ: String,
    pub address: String,
    pub pub_key: Option<PubKey>,
    pub account_number: Uint64,
    pub sequence: Uint64,
}

#[derive(Serialize, Deserialize)]
pub struct PubKey {
    #[serde(rename = "@type")]
    pub typ: String,
    pub key: Binary,
}

impl PubKey {
    #[allow(dead_code)]
    pub fn new(key: &[u8]) -> Self {
        PubKey {
            typ: "/cosmos.crypto.secp256k1.PubKey".to_string(),
            key: key.into(),
        }
    }
}

impl AuthAccount {
    pub fn new(addr: &str, sequence: u64) -> Self {
        AuthAccount {
            typ: "/cosmos.auth.v1beta1.BaseAccount".to_string(),
            address: addr.to_string(),
            pub_key: None,
            account_number: Uint64::new(4321),
            sequence: sequence.into(),
        }
    }
}

impl AuthAccountResponse {
    pub fn new(addr: &str, sequence: u64) -> Self {
        AuthAccountResponse {
            account: AuthAccount::new(addr, sequence),
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct Pagination {
    pub next_key: Option<String>,
    pub total: Uint64,
}

#[derive(Serialize, Deserialize)]
pub struct BalancesResponse {
    pub balances: Vec<Coin>,
    pub pagination: Pagination,
}

impl BalancesResponse {
    pub fn new(amount: u128) -> Self {
        BalancesResponse {
            balances: coins(amount, DEFAULT_DENOM),
            pagination: Pagination::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct GrantsResponse {
    // Placeholder... use a real type here if we want to present authz
    pub grants: Vec<Uint64>,
    pub pagination: Pagination,
}

#[derive(Serialize, Deserialize, Default)]
pub struct DelegationResponse {
    // Placeholder... use a real type here if we want to present authz
    pub delegation_responses: Vec<Uint64>,
    pub pagination: Pagination,
}

#[derive(Serialize, Deserialize, Default)]
pub struct UnbondingResponse {
    // Placeholder... use a real type here if we want to present authz
    pub unbonding_responses: Vec<Uint64>,
    pub pagination: Pagination,
}

#[derive(Serialize, Deserialize, Default)]
pub struct RewardsResponse {
    // Placeholder... use a real type here if we want to present authz
    pub rewards: Vec<Uint64>,
    pub total: Vec<Uint64>,
}

/* All the below provide some fake distribution/staking numbers to look good... replace when real values exist */

#[derive(Serialize, Deserialize)]
pub struct AnnualProvisionsResponse {
    pub annual_provisions: Decimal,
}

impl Default for AnnualProvisionsResponse {
    fn default() -> Self {
        // 20% of the default supply
        AnnualProvisionsResponse {
            annual_provisions: Decimal::from_ratio(DEFAULT_SUPPLY, 5u128),
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct PoolResponse {
    pub pool: Pool,
}

#[derive(Serialize, Deserialize)]
pub struct Pool {
    pub not_bonded_tokens: Uint128,
    pub bonded_tokens: Uint128,
}

impl Default for Pool {
    fn default() -> Self {
        Pool {
            not_bonded_tokens: Uint128::new(10_000_000_000),
            bonded_tokens: Uint128::new(90_000_000_000),
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct DistroParamsResponse {
    pub params: DistroParams,
}

#[derive(Serialize, Deserialize)]
pub struct DistroParams {
    pub community_tax: Decimal,
    pub base_proposer_reward: Decimal,
    pub bonus_proposer_reward: Decimal,
    pub withdraw_addr_enabled: bool,
}

impl Default for DistroParams {
    fn default() -> Self {
        DistroParams {
            community_tax: Decimal::percent(10),
            base_proposer_reward: Decimal::percent(1),
            bonus_proposer_reward: Decimal::percent(4),
            withdraw_addr_enabled: true,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct InflationResponse {
    pub inflation: Decimal,
}

impl Default for InflationResponse {
    fn default() -> Self {
        InflationResponse {
            inflation: Decimal::percent(20),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct SupplyResponse {
    pub amount: Coin,
}

impl SupplyResponse {
    pub fn new(denom: &str) -> Self {
        SupplyResponse {
            amount: coin(DEFAULT_SUPPLY, denom),
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct TransferResponse {
    pub params: TransferParams,
}

// Note: Default will set both as false, which is what we want (for now)
#[derive(Serialize, Deserialize, Default)]
pub struct TransferParams {
    pub send_enabled: bool,
    pub receive_enabled: bool,
}

#[derive(Serialize, Deserialize)]
pub struct SimulateResponse {
    pub gas_info: GasInfo,
    pub result: SimResult,
}

impl Default for SimulateResponse {
    fn default() -> Self {
        SimulateResponse {
            gas_info: GasInfo {
                gas_used: Uint64::new(123000),
                gas_wanted: Uint64::new(10000000),
            },
            result: SimResult {
                data: Binary::from(b"\x0a\x1e"),
                log: "[{\"events\":[]}]".to_string(),
                events: vec![],
            },
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct GasInfo {
    pub gas_wanted: Uint64,
    pub gas_used: Uint64,
}

#[derive(Serialize, Deserialize)]
pub struct SimResult {
    pub data: Binary,
    pub log: String,
    pub events: Vec<SimEvent>,
}

#[derive(Serialize, Deserialize)]
pub struct SimEvent {
    #[serde(rename = "type")]
    pub typ: String,
    pub attributes: Vec<SimAttr>,
}

#[derive(Serialize, Deserialize)]
pub struct SimAttr {
    pub key: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BroadcastResponse {
    pub tx_response: TxResponse,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TxResponse {
    pub height: Uint64,
    /// SHA-256 Transaction hash
    pub txhash: [u8; 32],
    pub data: Binary,
    pub code: u32,
    pub codespace: String,
    pub raw_log: String,
    pub gas_wanted: Uint64,
    pub gas_used: Uint64,
}

impl Default for BroadcastResponse {
    fn default() -> Self {
        BroadcastResponse {
            tx_response: TxResponse {
                txhash: [2u8; 32],
                height: Uint64::new(123),
                code: 0,
                codespace: "sdk".to_string(),
                data: Binary::from(b"\x0a\x1e"),
                raw_log: "[{\"events\":[]}]".to_string(),
                gas_used: Uint64::new(77000),
                gas_wanted: Uint64::new(123000),
            },
        }
    }
}
