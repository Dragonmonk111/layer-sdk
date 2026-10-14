//! jc-faucet — rate-limited testnet drip service for JunoClaw.
//!
//! Signs `cosmos.bank.v1beta1.MsgSend` transactions from the faucet account
//! and submits them via `cosmos.tx.v1beta1.Service/BroadcastTx` to a slay3rd
//! gRPC endpoint. Same signing path as `tx-sender` — the faucet key is
//! derived from SHA256($FAUCET_KEY_SEED) (deployer seed by default).
//!
//! Endpoints:
//!   GET  /health              -> {"ok":true, "address":..., "sequence":...}
//!   POST /drip {"address": "juno1..."}  -> {"txhash": "..."} or 429/400
//!
//! Rate limits (in-memory; restart resets):
//!   - per-address cooldown  (FAUCET_ADDR_COOLDOWN_SECS, default 6h)
//!   - per-IP cooldown       (FAUCET_IP_COOLDOWN_SECS,   default 10m)
//!   - global daily cap      (FAUCET_DAILY_CAP,          default 1000 drips)
//!
//! Env:
//!   FAUCET_GRPC          node gRPC host:port        (required)
//!   FAUCET_LISTEN        bind address             (default 0.0.0.0:8081)
//!   FAUCET_AMOUNT        drip amount in ujclaw    (default 1000000 = 1 JCLAW)
//!   FAUCET_KEY_SEED      key seed label           (default junoclaw-deployer-v1)
//!   FAUCET_CHAIN_ID      chain id                 (default junoclaw-1)
//!   FAUCET_ADDR_COOLDOWN_SECS / FAUCET_IP_COOLDOWN_SECS / FAUCET_DAILY_CAP

use std::collections::HashMap;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use cosmrs::crypto::secp256k1::SigningKey;
use cosmrs::tx::{Fee, SignDoc, SignerInfo};
use cosmrs::{AccountId, Coin};
use prost::Message;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tonic::transport::Channel;

use layer_proto::cosmos::auth::v1beta1::{BaseAccount, QueryAccountRequest, QueryAccountResponse};
use layer_proto::cosmos::bank::v1beta1::MsgSend;
use layer_proto::cosmos::base::v1beta1::Coin as ProtoCoin;
use layer_proto::cosmos::tx::v1beta1::{
    service_client::ServiceClient as TxServiceClient, BroadcastTxRequest,
};

const TYPE_URL_MSG_SEND: &str = "/cosmos.bank.v1beta1.MsgSend";
/// Fixed account number used by the chain (matches FIXED_ACCOUNT_NUMBER in layer_cosmos).
const FIXED_ACCOUNT_NUMBER: u64 = 17;
const BECH32_PREFIX: &str = "juno";
const FEE_DENOM: &str = "ujclaw";
const FEE_AMOUNT: u128 = 100_000;
const GAS_LIMIT: u64 = 200_000; // bank send only — no wasm

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}
fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

// ---------------------------------------------------------------------------
// Signing (mirrors tools/tx-sender)
// ---------------------------------------------------------------------------

fn faucet_seed(seed_label: &str) -> [u8; 32] {
    Sha256::digest(seed_label.as_bytes()).into()
}

fn faucet_key(seed: &[u8; 32]) -> SigningKey {
    SigningKey::from_slice(seed).expect("valid secp256k1 key from SHA256 seed")
}

fn sign_send_tx(
    seed: &[u8; 32],
    to: &str,
    amount: u128,
    sequence: u64,
    chain_id: &str,
) -> Vec<u8> {
    let key = faucet_key(seed);
    let from = key
        .public_key()
        .account_id(BECH32_PREFIX)
        .expect("valid bech32 address")
        .to_string();

    let msg = MsgSend {
        from_address: from,
        to_address: to.to_string(),
        amount: vec![ProtoCoin {
            denom: FEE_DENOM.to_string(),
            amount: amount.to_string(),
        }],
    };
    let any = cosmrs::Any {
        type_url: TYPE_URL_MSG_SEND.to_string(),
        value: msg.encode_to_vec(),
    };

    let fee_coin = Coin {
        amount: FEE_AMOUNT,
        denom: FEE_DENOM.parse().unwrap(),
    };
    let body = cosmrs::tx::Body::new(vec![any], "", 0u16);
    let signer_info = SignerInfo::single_direct(Some(key.public_key()), sequence);
    let auth_info = signer_info.auth_info(Fee::from_amount_and_gas(fee_coin, GAS_LIMIT));
    let doc = SignDoc::new(&body, &auth_info, &chain_id.parse().unwrap(), FIXED_ACCOUNT_NUMBER)
        .expect("valid sign doc");
    doc.sign(&key).expect("signing succeeded").to_bytes().expect("tx serialization")
}

async fn connect(grpc_addr: &str) -> Result<Channel, Box<dyn std::error::Error>> {
    Ok(Channel::from_shared(format!("http://{}", grpc_addr))?.connect().await?)
}

async fn query_sequence(grpc_addr: &str, address: &str) -> Result<u64, Box<dyn std::error::Error>> {
    let channel = connect(grpc_addr).await?;
    let mut client: tonic::client::Grpc<Channel> = tonic::client::Grpc::new(channel);
    client.ready().await?;
    let path: http::uri::PathAndQuery = "/cosmos.auth.v1beta1.Query/Account".parse().unwrap();
    let codec: tonic::codec::ProstCodec<QueryAccountRequest, QueryAccountResponse> =
        tonic::codec::ProstCodec::default();
    let resp = client
        .unary(
            tonic::Request::new(QueryAccountRequest {
                address: address.to_string(),
            }),
            path,
            codec,
        )
        .await?;
    match resp.into_inner().account {
        Some(any) => Ok(BaseAccount::decode(&any.value[..])
            .map(|a| a.sequence)
            .unwrap_or(0)),
        None => Ok(0),
    }
}

// ---------------------------------------------------------------------------
// Faucet state
// ---------------------------------------------------------------------------

struct RateLimit {
    per_address: HashMap<String, Instant>,
    per_ip: HashMap<String, Instant>,
    drips_today: u64,
    day_start: Instant,
}

struct FaucetState {
    key_seed: [u8; 32],
    address: String,
    grpc: String,
    chain_id: String,
    amount: u128,
    sequence: AtomicU64,
    seq_lock: tokio::sync::Mutex<()>,
    limits: Mutex<RateLimit>,
    addr_cooldown: Duration,
    ip_cooldown: Duration,
    daily_cap: u64,
}

#[derive(Deserialize)]
struct DripRequest {
    address: String,
}

#[derive(Serialize)]
struct DripResponse {
    txhash: String,
    amount: String,
    denom: String,
}

fn client_ip(headers: &HeaderMap, addr: &SocketAddr) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| addr.ip().to_string())
}

async fn health(State(st): State<Arc<FaucetState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ok": true,
        "faucet_address": st.address,
        "sequence": st.sequence.load(Ordering::Relaxed),
        "drip_amount": st.amount.to_string(),
        "denom": FEE_DENOM,
    }))
}

async fn drip(
    State(st): State<Arc<FaucetState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<DripRequest>,
) -> Result<Json<DripResponse>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let bad = |code: axum::http::StatusCode, msg: &str| {
        (code, Json(serde_json::json!({"error": msg})))
    };

    // Validate: proper juno1 bech32 address.
    let account = AccountId::from_str(req.address.trim())
        .map_err(|_| bad(axum::http::StatusCode::BAD_REQUEST, "invalid bech32 address"))?;
    if account.prefix() != BECH32_PREFIX {
        return Err(bad(
            axum::http::StatusCode::BAD_REQUEST,
            "address must use the juno prefix",
        ));
    }

    let ip = client_ip(&headers, &peer);
    let now = Instant::now();

    // Rate limits.
    {
        let mut rl = st.limits.lock().unwrap_or_else(|e| e.into_inner());
        if now.duration_since(rl.day_start) >= Duration::from_secs(86_400) {
            rl.day_start = now;
            rl.drips_today = 0;
        }
        if rl.drips_today >= st.daily_cap {
            return Err(bad(
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                "daily faucet cap reached — try again tomorrow",
            ));
        }
        if let Some(t) = rl.per_address.get(&req.address) {
            if now.duration_since(*t) < st.addr_cooldown {
                return Err(bad(
                    axum::http::StatusCode::TOO_MANY_REQUESTS,
                    "address cooldown — one drip per address per cooldown window",
                ));
            }
        }
        if let Some(t) = rl.per_ip.get(&ip) {
            if now.duration_since(*t) < st.ip_cooldown {
                return Err(bad(
                    axum::http::StatusCode::TOO_MANY_REQUESTS,
                    "IP cooldown — try again later",
                ));
            }
        }
    }

    // Sign + broadcast under the sequence lock so two drips can't collide.
    let _seq_guard = st.seq_lock.lock().await;
    let sequence = st.sequence.load(Ordering::Relaxed);
    let tx_bytes = sign_send_tx(&st.key_seed, &req.address, st.amount, sequence, &st.chain_id);
    let txhash = {
        let h = Sha256::digest(&tx_bytes);
        hex::encode_upper(h)
    };

    let mut client = TxServiceClient::new(
        connect(&st.grpc).await.map_err(|e| {
            bad(
                axum::http::StatusCode::BAD_GATEWAY,
                &format!("node unreachable: {e}"),
            )
        })?,
    );
    let resp = client
        .broadcast_tx(BroadcastTxRequest {
            tx_bytes,
            mode: 1, // BROADCAST_MODE_SYNC
        })
        .await
        .map_err(|e| {
            bad(
                axum::http::StatusCode::BAD_GATEWAY,
                &format!("broadcast failed: {e}"),
            )
        })?;

    if let Some(r) = resp.into_inner().tx_response {
        if r.code != 0 {
            // Sequence may have drifted — resync from chain on next drip.
            return Err(bad(
                axum::http::StatusCode::BAD_GATEWAY,
                &format!("check_tx rejected (code {}): {}", r.code, r.raw_log),
            ));
        }
    }

    st.sequence.fetch_add(1, Ordering::Relaxed);
    {
        let mut rl = st.limits.lock().unwrap_or_else(|e| e.into_inner());
        rl.per_address.insert(req.address.clone(), now);
        rl.per_ip.insert(ip, now);
        rl.drips_today += 1;
    }

    tracing::info!(to = %req.address, amount = st.amount, txhash = %txhash, "drip sent");
    Ok(Json(DripResponse {
        txhash,
        amount: st.amount.to_string(),
        denom: FEE_DENOM.to_string(),
    }))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let grpc = std::env::var("FAUCET_GRPC").expect("FAUCET_GRPC=host:port is required");
    let listen = env_or("FAUCET_LISTEN", "0.0.0.0:8081");
    let amount: u128 = env_or("FAUCET_AMOUNT", "1000000").parse().expect("FAUCET_AMOUNT must be an integer");
    let seed = env_or("FAUCET_KEY_SEED", "junoclaw-deployer-v1");
    let chain_id = env_or("FAUCET_CHAIN_ID", "junoclaw-1");
    let addr_cd = Duration::from_secs(env_u64("FAUCET_ADDR_COOLDOWN_SECS", 21_600));
    let ip_cd = Duration::from_secs(env_u64("FAUCET_IP_COOLDOWN_SECS", 600));
    let cap = env_u64("FAUCET_DAILY_CAP", 1000);

    let seed_bytes = faucet_seed(&seed);
    let address = faucet_key(&seed_bytes)
        .public_key()
        .account_id(BECH32_PREFIX)?
        .to_string();
    let sequence = query_sequence(&grpc, &address).await.unwrap_or_else(|e| {
        eprintln!("warning: initial sequence query failed ({e}) — starting at 0; first failed drip will resync");
        0
    });

    tracing::info!(
        grpc = %grpc,
        faucet = %address,
        sequence = sequence,
        amount = amount,
        "jc-faucet starting"
    );

    let state = Arc::new(FaucetState {
        key_seed: seed_bytes,
        address,
        grpc,
        chain_id,
        amount,
        sequence: AtomicU64::new(sequence),
        seq_lock: tokio::sync::Mutex::new(()),
        limits: Mutex::new(RateLimit {
            per_address: HashMap::new(),
            per_ip: HashMap::new(),
            drips_today: 0,
            day_start: Instant::now(),
        }),
        addr_cooldown: addr_cd,
        ip_cooldown: ip_cd,
        daily_cap: cap,
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/drip", post(drip))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&listen).await?;
    tracing::info!(listen = %listen, "jc-faucet listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}
