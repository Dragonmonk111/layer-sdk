//! jc-faucet — rate-limited testnet drip service for JunoClaw.
//!
//! Signs `cosmos.bank.v1beta1.MsgSend` transactions from the faucet account
//! and submits them via `cosmos.tx.v1beta1.Service/BroadcastTx` to a slay3rd
//! gRPC endpoint. Same signing path as `tx-sender` — the faucet key is
//! derived from SHA256($FAUCET_KEY_SEED) (deployer seed by default).
//!
//! Endpoints:
//!   GET  /health              -> {"ok":true, "address":..., "sequence":...}
//!   POST /drip {"address": "juno1..."}  -> {"txhash": "..."} (commit-verified) or 429/400/502/504
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
//!   FAUCET_MAYO_SK       MAYO secret-seed hex     (optional: enables hybrid
//!                        secp256k1+MAYO signing — the drip account becomes
//!                        the domain-separated hybrid address, which must be
//!                        funded separately before enabling)
//!   FAUCET_MAYO_VARIANT  mayo1|mayo2|mayo3|mayo5  (default mayo2)

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
use k256::ecdsa::{signature::hazmat::PrehashSigner, Signature as SecpSig, SigningKey as K256SigningKey};
use prost::Message;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tonic::transport::Channel;

use layer_proto::cosmos::auth::v1beta1::{BaseAccount, QueryAccountRequest, QueryAccountResponse};
use layer_proto::cosmos::bank::v1beta1::MsgSend;
use layer_proto::cosmos::base::v1beta1::Coin as ProtoCoin;
use layer_proto::cosmos::tx::signing::v1beta1::SignMode;
use layer_proto::cosmos::tx::v1beta1::{
    mode_info, service_client::ServiceClient as TxServiceClient, AuthInfo, BroadcastTxRequest,
    Fee as ProtoFee, GetTxRequest, ModeInfo, SignDoc as ProtoSignDoc, SignerInfo as ProtoSignerInfo,
    TxRaw,
};
use layer_proto::google::protobuf::Any;
use layer_std::{MayoVariant, PubKey, HYBRID_PUBKEY_TYPE_URL};

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

// ---------------------------------------------------------------------------
// Hybrid (PQ) signing — enabled by FAUCET_MAYO_SK + FAUCET_MAYO_VARIANT
// ---------------------------------------------------------------------------

/// Dual key material for HybridSecp256k1Mayo drips. When set, the faucet
/// account address is the domain-separated hybrid derivation — it must be
/// funded separately from the plain-secp256k1 account.
struct HybridSigner {
    secp: K256SigningKey,
    mayo_seed: Vec<u8>,
    variant: MayoVariant,
    pubkey_any_value: Vec<u8>,
    address: String,
}

fn parse_mayo_variant(s: &str) -> Result<MayoVariant, String> {
    match s {
        "mayo1" | "1" => Ok(MayoVariant::Mayo1),
        "mayo2" | "2" => Ok(MayoVariant::Mayo2),
        "mayo3" | "3" => Ok(MayoVariant::Mayo3),
        "mayo5" | "5" => Ok(MayoVariant::Mayo5),
        other => Err(format!("unknown mayo variant: {other}")),
    }
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.len() % 2 != 0 {
        return Err("hex string has odd length".to_string());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

/// Restore the MAYO public key from its compact secret seed.
fn mayo_pk_from_seed(variant: MayoVariant, seed: &[u8]) -> Result<Vec<u8>, String> {
    macro_rules! pk {
        ($p:ty) => {{
            let (pk, _sk) = sriracha_mayo::SecretKey::<$p>::from_seed(seed)
                .map_err(|e| format!("invalid mayo secret seed: {:?}", e))?;
            AsRef::<[u8]>::as_ref(&pk).to_vec()
        }};
    }
    Ok(match variant {
        MayoVariant::Mayo1 => pk!(sriracha_mayo::Mayo1),
        MayoVariant::Mayo2 => pk!(sriracha_mayo::Mayo2),
        MayoVariant::Mayo3 => pk!(sriracha_mayo::Mayo3),
        MayoVariant::Mayo5 => pk!(sriracha_mayo::Mayo5),
    })
}

/// MAYO-sign `message_hash` with the compact secret seed.
fn mayo_sign(variant: MayoVariant, seed: &[u8], message_hash: &[u8]) -> Result<Vec<u8>, String> {
    macro_rules! sign {
        ($p:ty) => {{
            let (_pk, sk) = sriracha_mayo::SecretKey::<$p>::from_seed(seed)
                .map_err(|e| format!("invalid mayo secret seed: {:?}", e))?;
            let sig = sk
                .sign(message_hash)
                .map_err(|e| format!("mayo signing failed: {:?}", e))?;
            AsRef::<[u8]>::as_ref(&sig).to_vec()
        }};
    }
    Ok(match variant {
        MayoVariant::Mayo1 => sign!(sriracha_mayo::Mayo1),
        MayoVariant::Mayo2 => sign!(sriracha_mayo::Mayo2),
        MayoVariant::Mayo3 => sign!(sriracha_mayo::Mayo3),
        MayoVariant::Mayo5 => sign!(sriracha_mayo::Mayo5),
    })
}

impl HybridSigner {
    fn new(seed: &[u8; 32]) -> Result<Option<Self>, String> {
        let mayo_sk = match std::env::var("FAUCET_MAYO_SK") {
            Ok(v) if !v.is_empty() => v,
            _ => return Ok(None),
        };
        let variant = parse_mayo_variant(&env_or("FAUCET_MAYO_VARIANT", "mayo2"))?;
        let mayo_seed = hex_decode(&mayo_sk)?;
        let secp =
            K256SigningKey::from_slice(seed).map_err(|e| format!("invalid secp key: {e}"))?;
        let mayo_pk = mayo_pk_from_seed(variant, &mayo_seed)?;
        let pk = PubKey::hybrid(secp.verifying_key().to_sec1_bytes().to_vec(), variant, mayo_pk);
        let pubkey_any_value = pk
            .to_hybrid_any_bytes()
            .ok_or("failed to encode hybrid pubkey")?;
        let address = pk
            .account_id()
            .map_err(|e| format!("hybrid account id: {e}"))?
            .to_string();
        Ok(Some(Self {
            secp,
            mayo_seed,
            variant,
            pubkey_any_value,
            address,
        }))
    }
}

/// Hybrid-signed MsgSend -> TxRaw bytes (mirrors hybrid-sign's emit path):
/// SignerInfo carries the hybrid Any pubkey; signature is [64B secp | MAYO]
/// over sha256(SignDoc).
fn sign_send_tx_hybrid(
    s: &HybridSigner,
    to: &str,
    amount: u128,
    sequence: u64,
    chain_id: &str,
) -> Result<Vec<u8>, String> {
    let msg = MsgSend {
        from_address: s.address.clone(),
        to_address: to.to_string(),
        amount: vec![ProtoCoin {
            denom: FEE_DENOM.to_string(),
            amount: amount.to_string(),
        }],
    };
    let tx_body = layer_proto::cosmos::tx::v1beta1::TxBody {
        messages: vec![Any {
            type_url: TYPE_URL_MSG_SEND.to_string(),
            value: msg.encode_to_vec(),
        }],
        memo: String::new(),
        timeout_height: 0,
        extension_options: vec![],
        non_critical_extension_options: vec![],
    };
    let body_bytes = tx_body.encode_to_vec();

    let auth_info = AuthInfo {
        signer_infos: vec![ProtoSignerInfo {
            public_key: Some(Any {
                type_url: HYBRID_PUBKEY_TYPE_URL.to_string(),
                value: s.pubkey_any_value.clone(),
            }),
            mode_info: Some(ModeInfo {
                sum: Some(mode_info::Sum::Single(mode_info::Single {
                    mode: SignMode::Direct as i32,
                })),
            }),
            sequence,
        }],
        fee: Some(ProtoFee {
            amount: vec![ProtoCoin {
                denom: FEE_DENOM.to_string(),
                amount: FEE_AMOUNT.to_string(),
            }],
            gas_limit: GAS_LIMIT,
            payer: String::new(),
            granter: String::new(),
        }),
        tip: None,
    };
    let auth_info_bytes = auth_info.encode_to_vec();

    let sign_doc = ProtoSignDoc {
        body_bytes: body_bytes.clone(),
        auth_info_bytes: auth_info_bytes.clone(),
        chain_id: chain_id.to_string(),
        account_number: FIXED_ACCOUNT_NUMBER,
    };
    let message_hash: [u8; 32] = Sha256::digest(sign_doc.encode_to_vec()).into();

    let secp_sig: SecpSig = s
        .secp
        .sign_prehash(&message_hash)
        .map_err(|e| format!("secp signing failed: {e}"))?;
    let mayo_sig = mayo_sign(s.variant, &s.mayo_seed, &message_hash)?;
    let packed = PubKey::pack_hybrid_signature(&secp_sig.to_bytes(), &mayo_sig);

    Ok(TxRaw {
        body_bytes,
        auth_info_bytes,
        signatures: vec![packed.to_vec()],
    }
    .encode_to_vec())
}

fn sign_send_tx(
    seed: &[u8; 32],
    hybrid: Option<&HybridSigner>,
    to: &str,
    amount: u128,
    sequence: u64,
    chain_id: &str,
) -> Vec<u8> {
    if let Some(h) = hybrid {
        return sign_send_tx_hybrid(h, to, amount, sequence, chain_id)
            .expect("hybrid signing failed");
    }
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

/// Re-read the account sequence from the chain and store it. The chain is
/// authoritative — txs signed elsewhere (or a committed drip) advance the
/// sequence out from under this process.
async fn resync_sequence(st: &FaucetState) -> Result<(), String> {
    let chain_seq = query_sequence(&st.grpc, &st.address)
        .await
        .map_err(|e| format!("sequence resync query failed: {e}"))?;
    st.sequence.store(chain_seq, Ordering::Relaxed);
    Ok(())
}

/// Poll GetTx until the tx is actually committed (height > 0) or the
/// deadline passes. A pending/mempool response carries height 0 — and on
/// this chain a pending response also reports code 0 (proto3 default),
/// so height is the commit signal, not code. Returns Err(()) on timeout
/// (tx may still commit later).
async fn wait_committed(
    client: &mut TxServiceClient<Channel>,
    txhash: &str,
    wait: Duration,
) -> Result<layer_proto::cosmos::base::abci::v1beta1::TxResponse, ()> {
    let deadline = Instant::now() + wait;
    loop {
        match client
            .get_tx(GetTxRequest {
                hash: txhash.to_string(),
            })
            .await
        {
            Ok(r) => match r.into_inner().tx_response {
                Some(t) if t.height > 0 => return Ok(t),
                Some(_) | None => {
                    if Instant::now() >= deadline {
                        return Err(());
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            },
            Err(s) if s.code() == tonic::Code::NotFound => {
                if Instant::now() >= deadline {
                    return Err(());
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            Err(_) => return Err(()),
        }
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
    hybrid: Option<HybridSigner>,
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

    let mut client = TxServiceClient::new(
        connect(&st.grpc).await.map_err(|e| {
            bad(
                axum::http::StatusCode::BAD_GATEWAY,
                &format!("node unreachable: {e}"),
            )
        })?,
    );

    // Up to two attempts: initial + one resync-retry. This chain's CheckTx
    // accepts stale-sequence txs (sequence is only verified at deliver time),
    // so a "successful" broadcast can still commit with code != 0. The drip
    // is only reported to the caller after the tx commits with code == 0.
    let mut txhash = String::new();
    for attempt in 0..2 {
        let sequence = st.sequence.load(Ordering::Relaxed);
        let tx_bytes = sign_send_tx(&st.key_seed, st.hybrid.as_ref(), &req.address, st.amount, sequence, &st.chain_id);
        txhash = {
            let h = Sha256::digest(&tx_bytes);
            hex::encode_upper(h)
        };

        let resp = match client
            .broadcast_tx(BroadcastTxRequest {
                tx_bytes,
                mode: 1, // BROADCAST_MODE_SYNC
            })
            .await
        {
            Ok(r) => r,
            Err(s) => {
                // gRPC-level CheckTx rejection (e.g. FailedPrecondition
                // InvalidSequence) — same resync-and-retry path as a
                // code-bearing rejection.
                let seq_err = s.code() == tonic::Code::FailedPrecondition
                    || s.message().contains("InvalidSequence")
                    || s.message().contains("doesn't match");
                if seq_err && attempt == 0 {
                    resync_sequence(st.as_ref()).await.map_err(|e| {
                        bad(axum::http::StatusCode::BAD_GATEWAY, &e)
                    })?;
                    continue;
                }
                return Err(bad(
                    axum::http::StatusCode::BAD_GATEWAY,
                    &format!("broadcast failed: {s}"),
                ));
            }
        };

        if let Some(r) = resp.into_inner().tx_response {
            if r.code != 0 {
                // CheckTx rejection — resync and retry once.
                if attempt == 0 {
                    resync_sequence(st.as_ref()).await.map_err(|e| {
                        bad(axum::http::StatusCode::BAD_GATEWAY, &e)
                    })?;
                    continue;
                }
                return Err(bad(
                    axum::http::StatusCode::BAD_GATEWAY,
                    &format!(
                        "check_tx rejected after resync (code {}): {}",
                        r.code, r.raw_log
                    ),
                ));
            }
        }

        // Broadcast accepted — now verify the tx actually commits.
        match wait_committed(&mut client, &txhash, Duration::from_secs(30)).await {
            Ok(r) if r.code == 0 => break, // confirmed on-chain
            Ok(r) => {
                // The faucet key is shared with the deployer account, so txs
                // signed elsewhere advance the chain sequence out from under
                // this process. The chain is authoritative — resync and retry
                // once on sequence mismatch; any other committed failure is
                // reported honestly instead of a fake success.
                if r.raw_log.contains("doesn't match") && attempt == 0 {
                    resync_sequence(st.as_ref()).await.map_err(|e| {
                        bad(axum::http::StatusCode::BAD_GATEWAY, &e)
                    })?;
                    continue;
                }
                return Err(bad(
                    axum::http::StatusCode::BAD_GATEWAY,
                    &format!(
                        "drip tx committed with error (code {}): {}",
                        r.code, r.raw_log
                    ),
                ));
            }
            Err(()) => {
                // Not committed within the wait window — the tx may still
                // land later. Resync the cached sequence from the chain (it
                // reflects the tx if it already committed) and report the
                // pending txhash honestly rather than claiming success.
                let _ = resync_sequence(st.as_ref()).await;
                return Err(bad(
                    axum::http::StatusCode::GATEWAY_TIMEOUT,
                    &format!(
                        "tx {txhash} accepted but not committed within 30s — check the hash before retrying"
                    ),
                ));
            }
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
    // Hybrid (PQ) mode: FAUCET_MAYO_SK + FAUCET_MAYO_VARIANT switch the
    // faucet account to the domain-separated hybrid address. That account
    // must be funded BEFORE the env is set — the plain-secp balance does
    // not carry over.
    let hybrid = HybridSigner::new(&seed_bytes).map_err(|e| format!("hybrid setup: {e}"))?;
    let address = match &hybrid {
        Some(h) => h.address.clone(),
        None => faucet_key(&seed_bytes)
            .public_key()
            .account_id(BECH32_PREFIX)?
            .to_string(),
    };
    if hybrid.is_some() {
        tracing::info!(address = %address, "hybrid (secp256k1+MAYO) drip mode enabled");
    }
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
        hybrid,
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
