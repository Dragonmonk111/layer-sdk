//! tx-sender — Cosmos tx builder and submitter for slay3r testnet.
//!
//! Derives a deterministic deployer secp256k1 key using the same seed as
//! `default_genesis()` in `app/slay3rd/src/main.rs`, constructs Cosmos proto-
//! encoded transactions (MsgStoreCode, MsgInstantiateContract, MsgExecuteContract),
//! signs them, and submits via BroadcastTx gRPC.
//!
//! Also queries state via Cosmos query paths (bank balances, auth account info,
//! CosmWasm SmartContractState) using tonic raw gRPC calls.
//!
//! # Deterministic deployer key
//!
//! Both tx-sender and `default_genesis()` derive the deployer key identically:
//!   1. Compute SHA256("junoclaw-deployer-v1") → 32 bytes
//!   2. Use those bytes as a secp256k1 signing key (cosmrs::crypto::secp256k1)
//!
//! # Usage
//!
//!   tx-sender balance [--grpc 127.0.0.1:9090] [--address juno1...]
//!   tx-sender store-code [--grpc 127.0.0.1:9090] --wasm path/to/contract.wasm [--sequence N]
//!   tx-sender instantiate [--grpc 127.0.0.1:9090] --code-id 1 [--label root] [--msg '{...}'] [--sequence N]
//!   tx-sender execute [--grpc 127.0.0.1:9090] --contract juno1... --msg '{...}' [--sequence N]
//!   tx-sender query [--grpc 127.0.0.1:9090] --contract juno1... [--msg '{}']

use cosmrs::{
    tx::{self, Fee, SignDoc, SignerInfo},
    AccountId, Coin,
};
use prost::Message;
use sha2::{Digest, Sha256};
use tonic::{
    codec::ProstCodec,
    transport::Channel,
    Request,
};

// Use layer-proto types which are compiled with prost 0.13 (workspace version),
// compatible with tonic 0.12 ProstCodec.
use layer_proto::cosmos::auth::v1beta1::{BaseAccount, QueryAccountRequest, QueryAccountResponse};
use layer_proto::cosmos::bank::v1beta1::{
    MsgSend, QueryAllBalancesRequest, QueryAllBalancesResponse,
};
use layer_proto::cosmos::base::v1beta1::Coin as ProtoCoin;
use layer_proto::cosmos::tx::v1beta1::{
    service_client::ServiceClient as TxServiceClient, BroadcastTxRequest,
};
use layer_proto::cosmwasm::wasm::v1::{
    MsgExecuteContract, MsgInstantiateContract, MsgStoreCode,
    QueryCodeRequest, QueryCodeResponse,
    QueryContractsByCodeRequest, QueryContractsByCodeResponse,
    QuerySmartContractStateRequest, QuerySmartContractStateResponse,
};

/// Chain constants — must match the running testnet.
const CHAIN_ID: &str = "junoclaw-1";
/// Fixed account number used by the Layer chain (matches FIXED_ACCOUNT_NUMBER in layer_cosmos).
const FIXED_ACCOUNT_NUMBER: u64 = 17;
/// Bech32 prefix for the Layer chain.
const BECH32_PREFIX: &str = "juno";
/// Gas limit for StoreCode (WASM upload needs generous gas).
// Must stay under the block gas cap (DEFAULT_BLOCK_GAS = 100_000_000) or the tx is
// rejected with ExceedsRemainingBlockGas before execution. The dominant cost is
// tx_byte_gas = tx_len * GAS_COST_TX_BYTE(10): a 4.4MB wasm tx needs ~44M gas just
// for its bytes. 90M covers up to ~9MB txs while leaving headroom under the cap.
const GAS_LIMIT: u64 = 90_000_000;
/// Fee amount in ujclaw.
const FEE_AMOUNT: u128 = 100_000;
/// Fee denomination.
const FEE_DENOM: &str = "ujclaw";

// Type URLs for CosmWasm messages — used to construct cosmrs::Any from raw proto bytes.
const TYPE_URL_MSG_STORE_CODE: &str = "/cosmwasm.wasm.v1.MsgStoreCode";
const TYPE_URL_MSG_INSTANTIATE_CONTRACT: &str = "/cosmwasm.wasm.v1.MsgInstantiateContract";
const TYPE_URL_MSG_EXECUTE_CONTRACT: &str = "/cosmwasm.wasm.v1.MsgExecuteContract";
const TYPE_URL_MSG_SEND: &str = "/cosmos.bank.v1beta1.MsgSend";

// ---------------------------------------------------------------------------
// CLI argument parsing (manual — no clap to avoid proc-macro2 version conflict)
// ---------------------------------------------------------------------------

fn print_usage() {
    eprintln!("Usage: tx-sender <subcommand> [flags]");
    eprintln!();
    eprintln!("Subcommands:");
    eprintln!("  balance     Query bank balance for deployer or given address");
    eprintln!("  store-code  Upload WASM bytecode (MsgStoreCode)");
    eprintln!("  instantiate Instantiate a contract (MsgInstantiateContract)");
    eprintln!("  execute     Execute a contract (MsgExecuteContract)");
    eprintln!("  send        Send tokens (MsgSend bank transfer)");
    eprintln!("  query       Query contract state (SmartContractState)");
    eprintln!("  contracts-by-code  List contract addresses for a code id");
    eprintln!("  code-info   Query CodeInfo for a code id");
    eprintln!();
    eprintln!("Common flags:");
    eprintln!("  --grpc <host:port>   gRPC server address (default: 127.0.0.1:9090)");
    eprintln!();
    eprintln!("store-code flags:");
    eprintln!("  --wasm <path>        Path to WASM file (required)");
    eprintln!("  --sequence <N>       Account sequence (default: auto-queried)");
    eprintln!();
    eprintln!("instantiate flags:");
    eprintln!("  --code-id <N>        Code ID to instantiate (required)");
    eprintln!("  --label <str>        Contract label (default: test-contract)");
    eprintln!("  --msg <json>         JSON instantiate message (default: auto gov_address)");
    eprintln!("  --sequence <N>       Account sequence (default: auto-queried)");
    eprintln!();
    eprintln!("execute flags:");
    eprintln!("  --contract <addr>    Contract address (required)");
    eprintln!("  --msg <json>         JSON execute message (required)");
    eprintln!("  --sequence <N>       Account sequence (default: auto-queried)");
    eprintln!();
    eprintln!("send flags:");
    eprintln!("  --to <addr>          Recipient address (required)");
    eprintln!("  --amount <N>         Amount to send (required)");
    eprintln!("  --denom <str>        Denom (default: ujclaw)");
    eprintln!("  --sequence <N>       Account sequence (default: auto-queried)");
    eprintln!();
    eprintln!("query flags:");
    eprintln!("  --contract <addr>    Contract address (required)");
    eprintln!("  --msg <json>         JSON query message (default: {{}})");
    eprintln!();
    eprintln!("contracts-by-code flags:");
    eprintln!("  --code-id <N>        Code ID to list contracts for (required)");
    eprintln!();
    eprintln!("code-info flags:");
    eprintln!("  --code-id <N>        Code ID to query (required)");
    eprintln!();
    eprintln!("balance flags:");
    eprintln!("  --address <addr>     Address to query (default: deployer address)");
}

struct Args {
    subcommand: String,
    grpc: String,
    wasm: Option<String>,
    sequence: Option<u64>,
    code_id: Option<u64>,
    label: String,
    msg: String,
    contract: Option<String>,
    address: Option<String>,
    to: Option<String>,
    amount: Option<String>,
    denom: String,
}

fn parse_args() -> Result<Args, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        print_usage();
        std::process::exit(0);
    }

    let subcommand = args[0].clone();
    let mut grpc = "127.0.0.1:9090".to_string();
    let mut wasm: Option<String> = None;
    let mut sequence: Option<u64> = None;
    let mut code_id: Option<u64> = None;
    let mut label = "test-contract".to_string();
    let mut msg = String::new();
    let mut contract: Option<String> = None;
    let mut address: Option<String> = None;
    let mut to: Option<String> = None;
    let mut amount: Option<String> = None;
    let mut denom = "ujclaw".to_string();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--grpc" => {
                i += 1;
                grpc = args.get(i).ok_or("--grpc requires a value")?.clone();
            }
            "--wasm" => {
                i += 1;
                wasm = Some(args.get(i).ok_or("--wasm requires a value")?.clone());
            }
            "--sequence" => {
                i += 1;
                let s = args.get(i).ok_or("--sequence requires a value")?;
                sequence = Some(s.parse().map_err(|_| format!("invalid sequence: {}", s))?);
            }
            "--code-id" => {
                i += 1;
                let s = args.get(i).ok_or("--code-id requires a value")?;
                code_id = Some(s.parse().map_err(|_| format!("invalid code-id: {}", s))?);
            }
            "--label" => {
                i += 1;
                label = args.get(i).ok_or("--label requires a value")?.clone();
            }
            "--msg" => {
                i += 1;
                msg = args.get(i).ok_or("--msg requires a value")?.clone();
            }
            "--msg-file" => {
                i += 1;
                let path = args.get(i).ok_or("--msg-file requires a value")?;
                msg = std::fs::read_to_string(path)
                    .map_err(|e| format!("failed to read --msg-file {}: {}", path, e))?
                    .trim()
                    .to_string();
            }
            "--contract" => {
                i += 1;
                contract = Some(args.get(i).ok_or("--contract requires a value")?.clone());
            }
            "--address" => {
                i += 1;
                address = Some(args.get(i).ok_or("--address requires a value")?.clone());
            }
            "--to" => {
                i += 1;
                to = Some(args.get(i).ok_or("--to requires a value")?.clone());
            }
            "--amount" => {
                i += 1;
                amount = Some(args.get(i).ok_or("--amount requires a value")?.clone());
            }
            "--denom" => {
                i += 1;
                denom = args.get(i).ok_or("--denom requires a value")?.clone();
            }
            flag => {
                return Err(format!("unknown flag: {}", flag));
            }
        }
        i += 1;
    }

    Ok(Args {
        subcommand,
        grpc,
        wasm,
        sequence,
        code_id,
        label,
        msg,
        contract,
        address,
        to,
        amount,
        denom,
    })
}

// ---------------------------------------------------------------------------
// Key derivation
// ---------------------------------------------------------------------------

/// Derive the deterministic deployer secp256k1 signing key.
///
/// Uses the same seed as `default_genesis()` in `app/slay3rd/src/main.rs`:
/// SHA256("junoclaw-deployer-v1")[0..32]
fn deployer_key() -> cosmrs::crypto::secp256k1::SigningKey {
    let seed = Sha256::digest(b"junoclaw-deployer-v1");
    cosmrs::crypto::secp256k1::SigningKey::from_slice(&seed[..32])
        .expect("valid secp256k1 key from SHA256 seed")
}

/// Derive the deployer bech32 address from the signing key.
fn deployer_address() -> AccountId {
    deployer_key()
        .public_key()
        .account_id(BECH32_PREFIX)
        .expect("valid bech32 address")
}

// ---------------------------------------------------------------------------
// Transaction signing
// ---------------------------------------------------------------------------

/// Build and sign a Cosmos tx containing one message.
///
/// The message bytes are pre-encoded proto (layer-proto types) wrapped in cosmrs::Any.
/// The transaction is signed with the deterministic deployer key.
/// Returns the raw serialized tx bytes ready for BroadcastTx.
fn sign_tx(msg: cosmrs::Any, sequence: u64) -> Vec<u8> {
    let chain_id = CHAIN_ID.parse().expect("valid chain id");
    let key = deployer_key();
    let pub_key = key.public_key();

    let fee_coin = Coin {
        amount: FEE_AMOUNT,
        denom: FEE_DENOM.parse().unwrap(),
    };

    let tx_body = tx::Body::new(vec![msg], "", 0u16);
    let signer_info = SignerInfo::single_direct(Some(pub_key), sequence);
    let auth_info = signer_info.auth_info(Fee::from_amount_and_gas(fee_coin, GAS_LIMIT));

    let sign_doc = SignDoc::new(&tx_body, &auth_info, &chain_id, FIXED_ACCOUNT_NUMBER)
        .expect("valid sign doc");
    let tx_signed = sign_doc.sign(&key).expect("signing succeeded");
    tx_signed.to_bytes().expect("tx serialization")
}

// ---------------------------------------------------------------------------
// gRPC channel creation
// ---------------------------------------------------------------------------

async fn connect(grpc_addr: &str) -> Result<Channel, Box<dyn std::error::Error>> {
    let channel = Channel::from_shared(format!("http://{}", grpc_addr))?
        .connect()
        .await?;
    Ok(channel)
}

// ---------------------------------------------------------------------------
// Account sequence query
// ---------------------------------------------------------------------------

/// Query the account sequence number for `address` via cosmos.auth.v1beta1.Query/Account.
///
/// Returns 0 if the account is not found (not yet on-chain).
async fn query_account_sequence(
    grpc_addr: &str,
    address: &str,
) -> Result<u64, Box<dyn std::error::Error>> {
    let req = QueryAccountRequest {
        address: address.to_string(),
    };

    let channel = connect(grpc_addr).await?;
    let mut grpc_client: tonic::client::Grpc<Channel> = tonic::client::Grpc::new(channel);
    grpc_client.ready().await.map_err(|e| {
        format!("gRPC channel not ready: {}", e)
    })?;

    let path: http::uri::PathAndQuery = "/cosmos.auth.v1beta1.Query/Account"
        .parse()
        .expect("valid path");
    let codec: ProstCodec<QueryAccountRequest, QueryAccountResponse> = ProstCodec::default();
    let response = grpc_client.unary(Request::new(req), path, codec).await?;

    let account_response = response.into_inner();
    if let Some(account_any) = account_response.account {
        match BaseAccount::decode(&account_any.value[..]) {
            Ok(base_account) => {
                println!(
                    "Account found: address={}, sequence={}",
                    base_account.address, base_account.sequence
                );
                Ok(base_account.sequence)
            }
            Err(e) => {
                println!(
                    "Warning: could not decode BaseAccount: {} — using sequence=0",
                    e
                );
                Ok(0)
            }
        }
    } else {
        println!("Account not found on-chain — using sequence=0");
        Ok(0)
    }
}

// ---------------------------------------------------------------------------
// Subcommand: store-code
// ---------------------------------------------------------------------------

async fn cmd_store_code(
    grpc_addr: &str,
    wasm_path: &str,
    explicit_sequence: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    let wasm_bytes = std::fs::read(wasm_path)?;
    println!("WASM bytecode size: {} bytes", wasm_bytes.len());

    let sender = deployer_address().to_string();
    println!("Deployer address: {}", sender);

    let sequence = match explicit_sequence {
        Some(seq) => {
            println!("Using explicit sequence: {}", seq);
            seq
        }
        None => query_account_sequence(grpc_addr, &sender).await?,
    };
    println!("Account sequence: {}", sequence);

    // Build MsgStoreCode using layer-proto types (prost 0.13 compatible)
    let msg_store_code = MsgStoreCode {
        sender: sender.clone(),
        wasm_byte_code: wasm_bytes,
    };
    // Encode proto bytes and wrap in cosmrs::Any for signing
    let proto_bytes = msg_store_code.encode_to_vec();
    let cosmrs_any = cosmrs::Any {
        type_url: TYPE_URL_MSG_STORE_CODE.to_string(),
        value: proto_bytes,
    };

    let tx_bytes = sign_tx(cosmrs_any, sequence);
    println!("Signed tx size: {} bytes", tx_bytes.len());

    let channel = connect(grpc_addr).await?;
    let mut client = TxServiceClient::new(channel)
        .max_decoding_message_size(10 * 1024 * 1024)
        .max_encoding_message_size(10 * 1024 * 1024);

    let response = client
        .broadcast_tx(BroadcastTxRequest {
            tx_bytes,
            mode: 1, // BROADCAST_MODE_SYNC
        })
        .await?;

    let tx_response = response.into_inner().tx_response;
    if let Some(ref resp) = tx_response {
        println!(
            "BroadcastTx response: code={}, log={}",
            resp.code, resp.raw_log
        );
        if resp.code == 0 {
            println!("StoreCode TX submitted successfully");
        } else {
            println!("Warning: tx returned non-zero code — check node logs");
        }
    } else {
        println!("StoreCode TX submitted (no response body)");
    }
    // On a fresh chain, the first code stored gets code_id=1
    println!("code_id=1 (assumed — first store on fresh chain)");

    Ok(())
}

// ---------------------------------------------------------------------------
// Subcommand: instantiate
// ---------------------------------------------------------------------------

async fn cmd_instantiate(
    grpc_addr: &str,
    code_id: u64,
    label: &str,
    msg_json: &str,
    explicit_sequence: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    let sender = deployer_address().to_string();
    println!("Deployer address: {}", sender);

    let sequence = match explicit_sequence {
        Some(seq) => {
            println!("Using explicit sequence: {}", seq);
            seq
        }
        None => query_account_sequence(grpc_addr, &sender).await?,
    };
    println!("Account sequence: {}", sequence);

    let msg = MsgInstantiateContract {
        sender: sender.clone(),
        admin: sender.clone(),
        code_id,
        label: label.to_string(),
        msg: msg_json.as_bytes().to_vec(),
        funds: vec![],
    };
    let proto_bytes = msg.encode_to_vec();
    let cosmrs_any = cosmrs::Any {
        type_url: TYPE_URL_MSG_INSTANTIATE_CONTRACT.to_string(),
        value: proto_bytes,
    };

    let tx_bytes = sign_tx(cosmrs_any, sequence);
    println!("Signed tx size: {} bytes", tx_bytes.len());

    let channel = connect(grpc_addr).await?;
    let mut client = TxServiceClient::new(channel)
        .max_decoding_message_size(10 * 1024 * 1024)
        .max_encoding_message_size(10 * 1024 * 1024);

    let response = client
        .broadcast_tx(BroadcastTxRequest {
            tx_bytes,
            mode: 1, // BROADCAST_MODE_SYNC
        })
        .await?;

    let tx_response = response.into_inner().tx_response;
    if let Some(ref resp) = tx_response {
        println!(
            "BroadcastTx response: code={}, log={}",
            resp.code, resp.raw_log
        );
        if resp.code == 0 {
            println!("InstantiateContract TX submitted successfully");
            println!("contract_address=<check node logs or query ContractsByCode>");
        } else {
            println!("Warning: tx returned non-zero code — check node logs");
        }
    } else {
        println!("InstantiateContract TX submitted (no response body)");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Subcommand: execute
// ---------------------------------------------------------------------------

async fn cmd_execute(
    grpc_addr: &str,
    contract: &str,
    msg_json: &str,
    explicit_sequence: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    let sender = deployer_address().to_string();
    println!("Deployer address: {}", sender);

    let sequence = match explicit_sequence {
        Some(seq) => {
            println!("Using explicit sequence: {}", seq);
            seq
        }
        None => query_account_sequence(grpc_addr, &sender).await?,
    };
    println!("Account sequence: {}", sequence);

    let msg = MsgExecuteContract {
        sender: sender.clone(),
        contract: contract.to_string(),
        msg: msg_json.as_bytes().to_vec(),
        funds: vec![],
    };
    let proto_bytes = msg.encode_to_vec();
    let cosmrs_any = cosmrs::Any {
        type_url: TYPE_URL_MSG_EXECUTE_CONTRACT.to_string(),
        value: proto_bytes,
    };

    let tx_bytes = sign_tx(cosmrs_any, sequence);
    println!("Signed tx size: {} bytes", tx_bytes.len());

    let channel = connect(grpc_addr).await?;
    let mut client = TxServiceClient::new(channel)
        .max_decoding_message_size(10 * 1024 * 1024)
        .max_encoding_message_size(10 * 1024 * 1024);

    let response = client
        .broadcast_tx(BroadcastTxRequest {
            tx_bytes,
            mode: 1, // BROADCAST_MODE_SYNC
        })
        .await?;

    let tx_response = response.into_inner().tx_response;
    if let Some(ref resp) = tx_response {
        println!(
            "BroadcastTx response: code={}, log={}",
            resp.code, resp.raw_log
        );
        if resp.code == 0 {
            println!("ExecuteContract TX submitted successfully");
        } else {
            println!("Warning: tx returned non-zero code — check node logs");
        }
    } else {
        println!("ExecuteContract TX submitted (no response body)");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Subcommand: send (MsgSend bank transfer)
// ---------------------------------------------------------------------------

async fn cmd_send(
    grpc_addr: &str,
    to: &str,
    amount: u128,
    denom: &str,
    explicit_sequence: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    let sender = deployer_address().to_string();
    println!("Deployer address: {}", sender);

    let sequence = match explicit_sequence {
        Some(seq) => {
            println!("Using explicit sequence: {}", seq);
            seq
        }
        None => query_account_sequence(grpc_addr, &sender).await?,
    };
    println!("Account sequence: {}", sequence);

    let msg = MsgSend {
        from_address: sender.clone(),
        to_address: to.to_string(),
        amount: vec![ProtoCoin {
            denom: denom.to_string(),
            amount: amount.to_string(),
        }],
    };
    let proto_bytes = msg.encode_to_vec();
    let cosmrs_any = cosmrs::Any {
        type_url: TYPE_URL_MSG_SEND.to_string(),
        value: proto_bytes,
    };

    let tx_bytes = sign_tx(cosmrs_any, sequence);
    println!("Signed tx size: {} bytes", tx_bytes.len());

    let channel = connect(grpc_addr).await?;
    let mut client = TxServiceClient::new(channel)
        .max_decoding_message_size(10 * 1024 * 1024)
        .max_encoding_message_size(10 * 1024 * 1024);

    let response = client
        .broadcast_tx(BroadcastTxRequest {
            tx_bytes,
            mode: 1, // BROADCAST_MODE_SYNC
        })
        .await?;

    let tx_response = response.into_inner().tx_response;
    if let Some(ref resp) = tx_response {
        println!(
            "BroadcastTx response: code={}, log={}",
            resp.code, resp.raw_log
        );
        if resp.code == 0 {
            println!("Send TX submitted successfully: {} {} -> {}", amount, denom, to);
        } else {
            println!("Warning: tx returned non-zero code — check node logs");
        }
    } else {
        println!("Send TX submitted (no response body)");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Subcommand: query (SmartContractState)
// ---------------------------------------------------------------------------

async fn cmd_query_smart(
    grpc_addr: &str,
    contract: &str,
    query_msg: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("[debug] query_data bytes ({}): {:?}", query_msg.len(), query_msg);
    let req = QuerySmartContractStateRequest {
        address: contract.to_string(),
        query_data: query_msg.as_bytes().to_vec(),
    };

    let channel = connect(grpc_addr).await?;
    let mut grpc_client: tonic::client::Grpc<Channel> = tonic::client::Grpc::new(channel);
    grpc_client.ready().await.map_err(|e| {
        format!("gRPC channel not ready: {}", e)
    })?;

    let path: http::uri::PathAndQuery = "/cosmwasm.wasm.v1.Query/SmartContractState"
        .parse()
        .expect("valid path");
    let codec: ProstCodec<QuerySmartContractStateRequest, QuerySmartContractStateResponse> =
        ProstCodec::default();
    let response = grpc_client.unary(Request::new(req), path, codec).await?;

    let result = response.into_inner();
    let data_str = String::from_utf8_lossy(&result.data);
    println!("SmartContractState result: {}", data_str);

    Ok(())
}

// ---------------------------------------------------------------------------
// Subcommand: contracts-by-code (ContractsByCode)
// ---------------------------------------------------------------------------

async fn cmd_contracts_by_code(
    grpc_addr: &str,
    code_id: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let req = QueryContractsByCodeRequest {
        code_id,
        pagination: None,
    };

    let channel = connect(grpc_addr).await?;
    let mut grpc_client: tonic::client::Grpc<Channel> = tonic::client::Grpc::new(channel);
    grpc_client.ready().await.map_err(|e| {
        format!("gRPC channel not ready: {}", e)
    })?;

    let path: http::uri::PathAndQuery = "/cosmwasm.wasm.v1.Query/ContractsByCode"
        .parse()
        .expect("valid path");
    let codec: ProstCodec<QueryContractsByCodeRequest, QueryContractsByCodeResponse> =
        ProstCodec::default();
    let response = grpc_client.unary(Request::new(req), path, codec).await?;

    let result = response.into_inner();
    if result.contracts.is_empty() {
        println!("No contracts found for code_id={}", code_id);
    } else {
        for addr in &result.contracts {
            println!("contract: {}", addr);
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Subcommand: code-info (Code)
// ---------------------------------------------------------------------------

async fn cmd_code_info(
    grpc_addr: &str,
    code_id: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let req = QueryCodeRequest { code_id };

    let channel = connect(grpc_addr).await?;
    let mut grpc_client: tonic::client::Grpc<Channel> = tonic::client::Grpc::new(channel)
        .max_decoding_message_size(10 * 1024 * 1024)
        .max_encoding_message_size(10 * 1024 * 1024);
    grpc_client.ready().await.map_err(|e| {
        format!("gRPC channel not ready: {}", e)
    })?;

    let path: http::uri::PathAndQuery = "/cosmwasm.wasm.v1.Query/Code"
        .parse()
        .expect("valid path");
    let codec: ProstCodec<QueryCodeRequest, QueryCodeResponse> =
        ProstCodec::default();
    let response = grpc_client.unary(Request::new(req), path, codec).await?;

    let result = response.into_inner();
    match result.code_info {
        Some(info) => {
            println!("code_id={} creator={} data_hash={} wasm_bytes={}",
                info.code_id, info.creator, hex::encode(&info.data_hash), result.data.len());
        }
        None => println!("No CodeInfo for code_id={} (wasm_bytes={})", code_id, result.data.len()),
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Subcommand: balance (AllBalances)
// ---------------------------------------------------------------------------

async fn cmd_query_balance(
    grpc_addr: &str,
    address: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let req = QueryAllBalancesRequest {
        address: address.to_string(),
        pagination: None,
    };

    let channel = connect(grpc_addr).await?;
    let mut grpc_client: tonic::client::Grpc<Channel> = tonic::client::Grpc::new(channel);
    grpc_client.ready().await.map_err(|e| {
        format!("gRPC channel not ready: {}", e)
    })?;

    let path: http::uri::PathAndQuery = "/cosmos.bank.v1beta1.Query/AllBalances"
        .parse()
        .expect("valid path");
    let codec: ProstCodec<QueryAllBalancesRequest, QueryAllBalancesResponse> =
        ProstCodec::default();
    let response = grpc_client.unary(Request::new(req), path, codec).await?;

    let result = response.into_inner();
    if result.balances.is_empty() {
        println!("Balance: [] (no funds or account not found)");
    } else {
        for coin in &result.balances {
            println!("Balance: {} {}", coin.amount, coin.denom);
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Error: {}", e);
            print_usage();
            std::process::exit(1);
        }
    };

    let result: Result<(), Box<dyn std::error::Error>> = match args.subcommand.as_str() {
        "store-code" => {
            let wasm = args.wasm.unwrap_or_else(|| {
                eprintln!("Error: --wasm is required for store-code");
                std::process::exit(1);
            });
            cmd_store_code(&args.grpc, &wasm, args.sequence).await
        }
        "instantiate" => {
            let code_id = args.code_id.unwrap_or_else(|| {
                eprintln!("Error: --code-id is required for instantiate");
                std::process::exit(1);
            });
            let deployer = deployer_address().to_string();
            // If no --msg provided, auto-generate with gov_address for the root contract.
            let msg_resolved = if args.msg.is_empty() || args.msg == "{}" {
                format!(r#"{{"gov_address":"{}"}}"#, deployer)
            } else {
                args.msg
            };
            cmd_instantiate(&args.grpc, code_id, &args.label, &msg_resolved, args.sequence).await
        }
        "execute" => {
            let contract = args.contract.unwrap_or_else(|| {
                eprintln!("Error: --contract is required for execute");
                std::process::exit(1);
            });
            let msg = if args.msg.is_empty() {
                eprintln!("Error: --msg is required for execute");
                std::process::exit(1);
            } else {
                args.msg
            };
            cmd_execute(&args.grpc, &contract, &msg, args.sequence).await
        }
        "query" => {
            let contract = args.contract.unwrap_or_else(|| {
                eprintln!("Error: --contract is required for query");
                std::process::exit(1);
            });
            let msg = if args.msg.is_empty() { "{}".to_string() } else { args.msg };
            cmd_query_smart(&args.grpc, &contract, &msg).await
        }
        "contracts-by-code" => {
            let code_id = args.code_id.unwrap_or_else(|| {
                eprintln!("Error: --code-id is required for contracts-by-code");
                std::process::exit(1);
            });
            cmd_contracts_by_code(&args.grpc, code_id).await
        }
        "code-info" => {
            let code_id = args.code_id.unwrap_or_else(|| {
                eprintln!("Error: --code-id is required for code-info");
                std::process::exit(1);
            });
            cmd_code_info(&args.grpc, code_id).await
        }
        "send" => {
            let to = args.to.unwrap_or_else(|| {
                eprintln!("Error: --to is required for send");
                std::process::exit(1);
            });
            let amount_str = args.amount.unwrap_or_else(|| {
                eprintln!("Error: --amount is required for send");
                std::process::exit(1);
            });
            let amount: u128 = amount_str.parse().unwrap_or_else(|_| {
                eprintln!("Error: invalid --amount: {}", amount_str);
                std::process::exit(1);
            });
            cmd_send(&args.grpc, &to, amount, &args.denom, args.sequence).await
        }
        "balance" => {
            let addr = args
                .address
                .unwrap_or_else(|| deployer_address().to_string());
            println!("Querying balance for: {}", addr);
            cmd_query_balance(&args.grpc, &addr).await
        }
        sub => {
            eprintln!("Unknown subcommand: {}", sub);
            print_usage();
            std::process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
