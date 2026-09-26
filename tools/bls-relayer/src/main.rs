//! bls-relayer — relays JunoClaw BLS finality to an 08-wasm light client on a
//! counterparty Cosmos chain (e.g. uni-7 / Juno testnet).
//!
//! JunoClaw side: queries `layer.lightclient.v1.Query` for proposal bytes,
//! certificates, timestamps, payloads, and Merkle membership proofs.
//!
//! Counterparty side: builds and broadcasts `MsgCreateClient`,
//! `MsgUpdateClient`, and gov `MsgSubmitProposal(MsgStoreCode)` — the ibc-go
//! wire types are hand-defined prost messages below (the repo does not vendor
//! ibc-go protos; these messages are small and stable across ibc-go v8+).
//!
//! # Usage
//!
//!   bls-relayer fetch --layer-grpc 127.0.0.1:9090 --height 42
//!   bls-relayer create-client --layer-grpc 127.0.0.1:9090 --grpc uni-7-grpc:9090 \
//!       --key-hex <secp256k1> --wasm light_client.wasm --height 42 \
//!       --group-pubkey-hex <96B> --chain-id junoclaw-1
//!   bls-relayer update-client --layer-grpc 127.0.0.1:9090 --grpc uni-7-grpc:9090 \
//!       --key-hex <secp256k1> --client-id 08-wasm-0 --height 43
//!   bls-relayer store-code --grpc uni-7-grpc:9090 --key-hex <secp256k1> \
//!       --wasm light_client.wasm --title "BLS light client" --summary "..."
//!   bls-relayer assemble-proof --layer-grpc 127.0.0.1:9090 --storage-key-hex <hex>

use cosmrs::{
    crypto::secp256k1::SigningKey,
    tendermint::chain::Id as ChainId,
    tx::{self, Fee, SignDoc, SignerInfo},
    Coin,
};
use futures::future::FutureExt;
use prost::Message;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tonic::{
    codec::ProstCodec,
    transport::{Channel, ClientTlsConfig},
    Request,
};

use layer_proto::cosmos::auth::v1beta1::{BaseAccount, QueryAccountRequest, QueryAccountResponse};
use layer_proto::cosmos::bank::v1beta1::{MsgSend, query_client::QueryClient as BankQueryClient, QueryBalanceRequest};
use layer_proto::cosmos::base::v1beta1::Coin as ProtoCoin;
use layer_proto::cosmos::tx::v1beta1::{
    service_client::ServiceClient as TxServiceClient, BroadcastMode, BroadcastTxRequest,
    GetTxRequest, SimulateRequest,
};
use layer_proto::cosmos::base::tendermint::v1beta1::{
    service_client::ServiceClient as TmServiceClient, GetLatestBlockRequest,
};
use layer_proto::cosmwasm::wasm::v1::{
    query_client::QueryClient as WasmQueryClient, MsgExecuteContract, MsgInstantiateContract,
    MsgStoreCode, QueryContractsByCodeRequest, QuerySmartContractStateRequest,
};
use layer_proto::layer::lightclient::v1::{
    query_client::QueryClient as LightClientQueryClient, QueryBlockRequest,
    QueryBlockResponse, QueryLatestHeightRequest, QueryProofRequest,
};

mod ibc;

use layer_std::IbcMsg;

// ---------------------------------------------------------------------------
// ibc-go wire types (hand-defined — wire-compatible with ibc-go v8+ protos)
// ---------------------------------------------------------------------------

/// ibc.core.client.v1.Height
#[derive(Clone, PartialEq, prost::Message)]
pub struct IbcHeight {
    #[prost(uint64, tag = "1")]
    pub revision_number: u64,
    #[prost(uint64, tag = "2")]
    pub revision_height: u64,
}

/// ibc.lightclients.wasm.v1.ClientState — wraps the contract's JSON state.
#[derive(Clone, PartialEq, prost::Message)]
pub struct WasmClientState {
    #[prost(bytes = "vec", tag = "1")]
    pub data: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub checksum: Vec<u8>,
    #[prost(message, optional, tag = "3")]
    pub latest_height: Option<IbcHeight>,
}

/// ibc.lightclients.wasm.v1.ConsensusState — wraps the contract's JSON state.
/// NOTE: ibc-go v8's proto has ONLY `data` (field 1) — no timestamp field.
/// The timestamp lives inside the contract's consensus-state JSON in `data`.
#[derive(Clone, PartialEq, prost::Message)]
pub struct WasmConsensusState {
    #[prost(bytes = "vec", tag = "1")]
    pub data: Vec<u8>,
}

/// ibc.lightclients.wasm.v1.ClientMessage — wraps the contract's Header JSON.
#[derive(Clone, PartialEq, prost::Message)]
pub struct WasmClientMessage {
    #[prost(bytes = "vec", tag = "1")]
    pub data: Vec<u8>,
}

/// ibc.core.client.v1.MsgCreateClient
#[derive(Clone, PartialEq, prost::Message)]
pub struct MsgCreateClient {
    #[prost(message, optional, tag = "1")]
    pub client_state: Option<prost_types::Any>,
    #[prost(message, optional, tag = "2")]
    pub consensus_state: Option<prost_types::Any>,
    #[prost(string, tag = "3")]
    pub signer: String,
}

/// ibc.core.client.v1.MsgUpdateClient
#[derive(Clone, PartialEq, prost::Message)]
pub struct MsgUpdateClient {
    #[prost(string, tag = "1")]
    pub client_id: String,
    #[prost(message, optional, tag = "2")]
    pub client_message: Option<prost_types::Any>,
    #[prost(string, tag = "3")]
    pub signer: String,
}

/// ibc.lightclients.wasm.v1.MsgStoreCode — stores a light-client contract
/// in the ibcwasm module store. Authority-gated: `signer` must be the gov
/// module account, so it only executes inside a passed gov proposal.
#[derive(Clone, PartialEq, prost::Message)]
pub struct IbcWasmMsgStoreCode {
    #[prost(string, tag = "1")]
    pub signer: String,
    #[prost(bytes = "vec", tag = "2")]
    pub wasm_byte_code: Vec<u8>,
}

/// cosmos.gov.v1.MsgSubmitProposal (SDK 0.47+ gov v1)
#[derive(Clone, PartialEq, prost::Message)]
pub struct MsgSubmitProposal {
    #[prost(message, repeated, tag = "1")]
    pub messages: Vec<prost_types::Any>,
    #[prost(message, repeated, tag = "2")]
    pub initial_deposit: Vec<ProtoCoin>,
    #[prost(string, tag = "3")]
    pub proposer: String,
    #[prost(string, tag = "4")]
    pub metadata: String,
    #[prost(string, tag = "5")]
    pub title: String,
    #[prost(string, tag = "6")]
    pub summary: String,
    #[prost(bool, tag = "7")]
    pub expedited: bool,
}

// Type URLs
const TYPE_URL_WASM_CLIENT_STATE: &str = "/ibc.lightclients.wasm.v1.ClientState";
const TYPE_URL_WASM_CONSENSUS_STATE: &str = "/ibc.lightclients.wasm.v1.ConsensusState";
const TYPE_URL_WASM_CLIENT_MESSAGE: &str = "/ibc.lightclients.wasm.v1.ClientMessage";
const TYPE_URL_MSG_CREATE_CLIENT: &str = "/ibc.core.client.v1.MsgCreateClient";
const TYPE_URL_MSG_UPDATE_CLIENT: &str = "/ibc.core.client.v1.MsgUpdateClient";
const TYPE_URL_MSG_STORE_CODE: &str = "/cosmwasm.wasm.v1.MsgStoreCode";
const TYPE_URL_MSG_INSTANTIATE_CONTRACT: &str = "/cosmwasm.wasm.v1.MsgInstantiateContract";
const TYPE_URL_MSG_EXECUTE_CONTRACT: &str = "/cosmwasm.wasm.v1.MsgExecuteContract";
const TYPE_URL_IBCWASM_MSG_STORE_CODE: &str = "/ibc.lightclients.wasm.v1.MsgStoreCode";
const TYPE_URL_MSG_SUBMIT_PROPOSAL: &str = "/cosmos.gov.v1.MsgSubmitProposal";

/// Revision number stamped on every IBC `Height` this relayer emits.
///
/// WORKAROUND: ibc-go marshals `clienttypes.Height` with `omitempty`, so a
/// `revision_number` of 0 is dropped from the `VerifyMembership`/`UpdateState`
/// sudo payloads. The currently-deployed 08-wasm contract declares the field
/// required (no `serde(default)`), so a 0 revision fails to deserialize. Using
/// a non-zero revision keeps the field present end-to-end; the contract only
/// asserts `header.revision_number == latest_height.revision_number`, so a
/// consistent non-zero value is sufficient. Set back to 0 once the
/// `#[serde(default)]` contract build is stored on-chain.
const HEIGHT_REVISION_NUMBER: u64 = 1;

// ---------------------------------------------------------------------------
// Contract JSON mirrors — must match contracts/light-client serde exactly
// (Binary fields serialize as base64 strings, Option<Binary> as b64-or-null)
// ---------------------------------------------------------------------------

mod b64 {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(v))
    }
    #[allow(dead_code)]
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        STANDARD.decode(&s).map_err(serde::de::Error::custom)
    }
}

mod b64_opt_vec {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[Option<Vec<u8>>], s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(v.iter().map(|o| o.as_ref().map(|b| STANDARD.encode(b))))
    }
    #[allow(dead_code)]
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Option<Vec<u8>>>, D::Error> {
        let v: Vec<Option<String>> = Vec::deserialize(d)?;
        v.into_iter()
            .map(|o| {
                o.map(|s| STANDARD.decode(&s).map_err(serde::de::Error::custom))
                    .transpose()
            })
            .collect()
    }
}

/// Contract `Height` (state.rs)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContractHeight {
    pub revision_number: u64,
    pub revision_height: u64,
}

/// Contract `ClientState` (state.rs) — stored as `WasmClientState.data`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContractClientState {
    pub chain_id: String,
    pub group_public_key_hex: String,
    pub latest_height: ContractHeight,
    pub frozen_height: Option<ContractHeight>,
}

/// Contract `ConsensusState` (state.rs) — stored as `WasmConsensusState.data`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContractConsensusState {
    pub payload_digest_hex: String,
    pub timestamp: u64,
    pub epoch: u64,
    pub view: u64,
    pub parent: u64,
}

/// Contract `Header` (msg.rs) — stored as `WasmClientMessage.data`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContractHeader {
    pub height: ContractHeight,
    pub timestamp: u64,
    #[serde(with = "b64")]
    pub proposal_bytes: Vec<u8>,
    #[serde(with = "b64")]
    pub certificate_bytes: Vec<u8>,
}

/// Contract `MembershipProof` (msg.rs) — the JSON inside `proof` bytes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContractMembershipProof {
    #[serde(with = "b64")]
    pub payload_bytes: Vec<u8>,
    pub leaf_index: u64,
    #[serde(with = "b64_opt_vec")]
    pub siblings: Vec<Option<Vec<u8>>>,
}

// ---------------------------------------------------------------------------
// Proposal decoding — commonware-codec layout:
//   uvarint(epoch) || uvarint(view) || uvarint(parent) || payload(32 bytes)
// (mirrors contracts/light-client/src/verify.rs::decode_proposal)
// ---------------------------------------------------------------------------

struct DecodedProposal {
    epoch: u64,
    view: u64,
    parent: u64,
    payload: [u8; 32],
}

fn read_uvarint(buf: &[u8], pos: &mut usize) -> Result<u64, String> {
    let mut value: u64 = 0;
    let mut shift = 0u32;
    loop {
        let byte = *buf.get(*pos).ok_or("varint: unexpected end of input")?;
        *pos += 1;
        if shift == 63 && byte > 1 {
            return Err("varint: overflow".into());
        }
        value |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
    }
}

fn decode_proposal(bytes: &[u8]) -> Result<DecodedProposal, String> {
    let mut pos = 0usize;
    let epoch = read_uvarint(bytes, &mut pos)?;
    let view = read_uvarint(bytes, &mut pos)?;
    let parent = read_uvarint(bytes, &mut pos)?;
    let payload: [u8; 32] = bytes
        .get(pos..pos + 32)
        .ok_or("proposal: missing 32-byte payload digest")?
        .try_into()
        .unwrap();
    Ok(DecodedProposal {
        epoch,
        view,
        parent,
        payload,
    })
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

struct Args {
    cmd: String,
    layer_grpc: String,
    grpc: String,
    key_hex: Option<String>,
    mnemonic: Option<String>,
    to: Option<String>,
    amount: Option<String>,
    wasm: Option<String>,
    height: Option<u64>,
    client_id: Option<String>,
    group_pubkey_hex: Option<String>,
    chain_id: String,
    cp_chain_id: String,
    fee_denom: String,
    fee_amount: u128,
    gas: u64,
    title: Option<String>,
    summary: Option<String>,
    deposit: Option<String>,
    storage_key_hex: Option<String>,
    bech32_prefix: String,
    direct: bool,
    ibcwasm: bool,
    simulate: bool,
    sequence: Option<u64>,
    // IBC handshake / transfer fields
    jc_key_hex: Option<String>,
    connection_id: Option<String>,
    port_id: Option<String>,
    channel_id: Option<String>,
    cp_client_id: Option<String>,
    cp_connection_id: Option<String>,
    cp_port_id: Option<String>,
    cp_channel_id: Option<String>,
    ordering: Option<String>,
    version: Option<String>,
    proof_height: Option<u64>,
    timeout_height: Option<u64>,
    timeout_timestamp: Option<u64>,
    // relay daemon
    interval: u64,
    update_cadence: u64,
    min_fee_balance: u128,
    health_addr: Option<String>,
    max_retries: u32,
    // JunoClaw wasm deploy
    code_id: Option<u64>,
    contract: Option<String>,
    msg_json: Option<String>,
    label: Option<String>,
}

fn print_usage() {
    eprintln!("Usage: bls-relayer <subcommand> [flags]");
    eprintln!();
    eprintln!("Subcommands:");
    eprintln!("  fetch           Fetch block header components from the JunoClaw node");
    eprintln!("  create-client   Create the 08-wasm client on the counterparty chain");
    eprintln!("  update-client   Submit a new finalized header to the client");
    eprintln!("  store-code      Gov-propose the contract wasm on the counterparty");
    eprintln!("  assemble-proof  Build a MembershipProof JSON for a storage key
  send            MsgSend tokens on the counterparty (fund the relayer key)
  keygen          Generate a secp256k1 key + print the juno address
  account         Query account_number/sequence on the counterparty");
    eprintln!();
    eprintln!("JunoClaw sovereign IBC (signs with deployer key, or --jc-key-hex):");
    eprintln!("  jc-create-client  Register a nominal client for the counterparty");
    eprintln!("  jc-conn-init      ConnectionOpenInit on JunoClaw");
    eprintln!("  jc-conn-ack       ConnectionOpenAck on JunoClaw (INIT->OPEN)");
    eprintln!("  jc-chan-init      ChannelOpenInit on JunoClaw");
    eprintln!("  jc-chan-ack       ChannelOpenAck on JunoClaw (INIT->OPEN)");
    eprintln!("  jc-transfer       ICS-20 transfer (escrow + packet commitment)");
    eprintln!("  jc-ack            Acknowledgement (clear packet commitment)");
    eprintln!();
    eprintln!("JunoClaw CosmWasm (signs with deployer key, or --jc-key-hex):");
    eprintln!("  jc-store-code     MsgStoreCode — upload a .wasm to JunoClaw");
    eprintln!("  jc-instantiate    MsgInstantiateContract --code-id <n> [--label <l>] [--msg <json>]");
    eprintln!("  jc-execute        MsgExecuteContract --contract <addr> --msg <json|@file>");
    eprintln!("  jc-query          Query/SmartContractState --contract <addr> --msg <json|@file>");
    eprintln!("  jc-contracts      Query/ContractsByCode --code-id <n>");
    eprintln!("                    (--msg @path reads the JSON from a file)");
    eprintln!();
    eprintln!("Counterparty ibc-go handshake + packet relay (signs with --key-hex):");
    eprintln!("  conn-try          MsgConnectionOpenTry (proof of JunoClaw conn INIT)");
    eprintln!("  conn-confirm      MsgConnectionOpenConfirm (proof of JunoClaw conn OPEN)");
    eprintln!("  chan-try          MsgChannelOpenTry (proof of JunoClaw chan INIT)");
    eprintln!("  chan-confirm      MsgChannelOpenConfirm (proof of JunoClaw chan OPEN)");
    eprintln!("  recv-packet       MsgRecvPacket (proof of JunoClaw packet commitment)");
    eprintln!("  relay             Daemon: auto update-client + relay/ack pending packets");
    eprintln!();
    eprintln!("Common flags:");
    eprintln!("  --layer-grpc <host:port>   JunoClaw gRPC (default 127.0.0.1:9090)");
    eprintln!("  --grpc <host:port>         Counterparty gRPC (default 127.0.0.1:9190)");
    eprintln!("  --key-hex <hex>            Counterparty secp256k1 private key (or RELAYER_KEY_HEX env)");
    eprintln!("  --mnemonic <words>         Counterparty BIP39 mnemonic (or RELAYER_MNEMONIC env)");
    eprintln!("  --cp-chain-id <id>         Counterparty chain id (default uni-7)");
    eprintln!("  --bech32-prefix <p>        Counterparty bech32 prefix (default juno)");
    eprintln!("  --fee-denom <d>            Fee denom (default ujunox)");
    eprintln!("  --fee-amount <n>           Fee amount (default 5000)");
    eprintln!("  --gas <n>                  Gas limit (default 4000000)");
    eprintln!();
    eprintln!("fetch:            --height <N>");
    eprintln!("create-client:    --height <N> --wasm <path> --group-pubkey-hex <hex> --chain-id <id>");
    eprintln!("update-client:    --height <N> --client-id <id>");
    eprintln!("store-code:       --wasm <path> --title <t> --summary <s> [--deposit <amt><denom>]");
    eprintln!("assemble-proof:   --storage-key-hex <hex>");
    eprintln!("send:             --to <addr> --amount <n><denom>");
    eprintln!();
    eprintln!("IBC proof commands auto-update the 08-wasm client to proof_height first:");
    eprintln!("  conn-try:       --client-id <08-wasm-N> --cp-client-id <id> --cp-connection-id <id>");
    eprintln!("  conn-confirm:   --client-id <id> --connection-id <id> --cp-connection-id <id>");
    eprintln!("  chan-try:       --client-id <id> --connection-id <id> --cp-channel-id <id> [--cp-port-id <p>] [--version <v>]");
    eprintln!("  chan-confirm:   --client-id <id> --channel-id <id> --cp-channel-id <id> [--cp-port-id <p>]");
    eprintln!("  recv-packet:    --client-id <id> --channel-id <id> --cp-channel-id <id> --sequence <n> --to <rcpt> --amount <n><denom>");
    eprintln!();
    eprintln!("relay (daemon, JunoClaw-centric: --channel-id = JunoClaw source, --cp-channel-id = dest):");
    eprintln!("  relay:          --client-id <08-wasm-N> --channel-id <jc-chan> --cp-channel-id <cp-chan>");
    eprintln!("                  [--interval <s>] [--update-cadence <blocks>] [--max-retries <n>]");
    eprintln!("                  [--min-fee-balance <n>] [--health-addr <host:port>]");
}

fn parse_args() -> Result<Args, String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        return Err("missing subcommand".into());
    }
    let mut a = Args {
        cmd: args[1].clone(),
        layer_grpc: "127.0.0.1:9090".into(),
        grpc: "127.0.0.1:9190".into(),
        key_hex: std::env::var("RELAYER_KEY_HEX").ok(),
        mnemonic: std::env::var("RELAYER_MNEMONIC").ok(),
        to: None,
        amount: None,
        wasm: None,
        height: None,
        client_id: None,
        group_pubkey_hex: None,
        chain_id: "junoclaw-1".into(),
        cp_chain_id: "uni-7".into(),
        fee_denom: "ujunox".into(),
        fee_amount: 5000,
        gas: 4_000_000,
        title: None,
        summary: None,
        deposit: None,
        storage_key_hex: None,
        bech32_prefix: "juno".into(),
        direct: false,
        ibcwasm: false,
        simulate: false,
        sequence: None,
        jc_key_hex: std::env::var("JUNOCLAW_KEY_HEX").ok(),
        connection_id: None,
        port_id: None,
        channel_id: None,
        cp_client_id: None,
        cp_connection_id: None,
        cp_port_id: None,
        cp_channel_id: None,
        ordering: None,
        version: None,
        proof_height: None,
        timeout_height: None,
        timeout_timestamp: None,
        interval: 6,
        update_cadence: 50,
        min_fee_balance: 0,
        health_addr: None,
        max_retries: 60,
        code_id: None,
        contract: None,
        msg_json: None,
        label: None,
    };
    let mut i = 2;
    while i < args.len() {
        let flag = args[i].as_str();
        let val = |i: &mut usize| -> Result<String, String> {
            *i += 1;
            args.get(*i)
                .cloned()
                .ok_or_else(|| format!("{flag} requires a value"))
        };
        match flag {
            "--layer-grpc" => a.layer_grpc = val(&mut i)?,
            "--grpc" => a.grpc = val(&mut i)?,
            "--key-hex" => a.key_hex = Some(val(&mut i)?),
            "--mnemonic" => a.mnemonic = Some(val(&mut i)?),
            "--to" => a.to = Some(val(&mut i)?),
            "--amount" => a.amount = Some(val(&mut i)?),
            "--wasm" => a.wasm = Some(val(&mut i)?),
            "--height" => {
                a.height = Some(val(&mut i)?.parse().map_err(|_| "invalid --height")?)
            }
            "--client-id" => a.client_id = Some(val(&mut i)?),
            "--group-pubkey-hex" => a.group_pubkey_hex = Some(val(&mut i)?),
            "--chain-id" => a.chain_id = val(&mut i)?,
            "--cp-chain-id" => a.cp_chain_id = val(&mut i)?,
            "--fee-denom" => a.fee_denom = val(&mut i)?,
            "--fee-amount" => {
                a.fee_amount = val(&mut i)?.parse().map_err(|_| "invalid --fee-amount")?
            }
            "--gas" => a.gas = val(&mut i)?.parse().map_err(|_| "invalid --gas")?,
            "--title" => a.title = Some(val(&mut i)?),
            "--summary" => a.summary = Some(val(&mut i)?),
            "--deposit" => a.deposit = Some(val(&mut i)?),
            "--storage-key-hex" => a.storage_key_hex = Some(val(&mut i)?),
            "--bech32-prefix" => a.bech32_prefix = val(&mut i)?,
            "--direct" => a.direct = true,
            "--ibcwasm" => a.ibcwasm = true,
            "--simulate" => a.simulate = true,
            "--sequence" => {
                a.sequence = Some(val(&mut i)?.parse().map_err(|_| "invalid --sequence")?)
            }
            "--jc-key-hex" => a.jc_key_hex = Some(val(&mut i)?),
            "--connection-id" => a.connection_id = Some(val(&mut i)?),
            "--port-id" => a.port_id = Some(val(&mut i)?),
            "--channel-id" => a.channel_id = Some(val(&mut i)?),
            "--cp-client-id" => a.cp_client_id = Some(val(&mut i)?),
            "--cp-connection-id" => a.cp_connection_id = Some(val(&mut i)?),
            "--cp-port-id" => a.cp_port_id = Some(val(&mut i)?),
            "--cp-channel-id" => a.cp_channel_id = Some(val(&mut i)?),
            "--ordering" => a.ordering = Some(val(&mut i)?),
            "--version" => a.version = Some(val(&mut i)?),
            "--proof-height" => {
                a.proof_height =
                    Some(val(&mut i)?.parse().map_err(|_| "invalid --proof-height")?)
            }
            "--timeout-height" => {
                a.timeout_height =
                    Some(val(&mut i)?.parse().map_err(|_| "invalid --timeout-height")?)
            }
            "--timeout-timestamp" => {
                a.timeout_timestamp = Some(
                    val(&mut i)?
                        .parse()
                        .map_err(|_| "invalid --timeout-timestamp")?,
                )
            }
            "--interval" => {
                a.interval = val(&mut i)?.parse().map_err(|_| "invalid --interval")?
            }
            "--update-cadence" => {
                a.update_cadence = val(&mut i)?
                    .parse()
                    .map_err(|_| "invalid --update-cadence")?
            }
            "--min-fee-balance" => {
                a.min_fee_balance = val(&mut i)?
                    .parse()
                    .map_err(|_| "invalid --min-fee-balance")?
            }
            "--health-addr" => a.health_addr = Some(val(&mut i)?),
            "--max-retries" => {
                a.max_retries = val(&mut i)?
                    .parse()
                    .map_err(|_| "invalid --max-retries")?
            }
            "--code-id" => {
                a.code_id = Some(val(&mut i)?.parse().map_err(|_| "invalid --code-id")?)
            }
            "--contract" => a.contract = Some(val(&mut i)?),
            "--msg" => a.msg_json = Some(val(&mut i)?),
            "--label" => a.label = Some(val(&mut i)?),
            other => return Err(format!("unknown flag: {other}")),
        }
        i += 1;
    }
    Ok(a)
}

// ---------------------------------------------------------------------------
// gRPC helpers
// ---------------------------------------------------------------------------

async fn connect(addr: &str) -> Result<Channel, Box<dyn std::error::Error>> {
    // Scheme handling: "https://" (or bare host ending in :443) → TLS with
    // native roots; "http://" or bare host:port → plaintext (local node).
    let (url, tls) = if addr.starts_with("https://") {
        (addr.to_string(), true)
    } else if addr.starts_with("http://") {
        (addr.to_string(), false)
    } else if addr.ends_with(":443") {
        (format!("https://{addr}"), true)
    } else {
        (format!("http://{addr}"), false)
    };
    let mut endpoint = Channel::from_shared(url)?;
    if tls {
        endpoint = endpoint.tls_config(ClientTlsConfig::new().with_native_roots())?;
    }
    Ok(endpoint.connect().await?)
}

async fn layer_client(
    addr: &str,
) -> Result<LightClientQueryClient<Channel>, Box<dyn std::error::Error>> {
    Ok(LightClientQueryClient::new(connect(addr).await?)
        .max_decoding_message_size(32 * 1024 * 1024)
        .max_encoding_message_size(32 * 1024 * 1024))
}

struct AccountInfo {
    account_number: u64,
    sequence: u64,
}

/// Query account_number + sequence via cosmos.auth.v1beta1.Query/Account.
async fn query_account(
    grpc: &str,
    address: &str,
) -> Result<AccountInfo, Box<dyn std::error::Error>> {
    let channel = connect(grpc).await?;
    let mut client: tonic::client::Grpc<Channel> = tonic::client::Grpc::new(channel);
    client.ready().await?;
    let path: http::uri::PathAndQuery = "/cosmos.auth.v1beta1.Query/Account".parse()?;
    let codec: ProstCodec<QueryAccountRequest, QueryAccountResponse> = ProstCodec::default();
    let resp = client
        .unary(
            Request::new(QueryAccountRequest {
                address: address.to_string(),
            }),
            path,
            codec,
        )
        .await?
        .into_inner();
    let any = resp.account.ok_or("account query returned no account")?;
    let base = BaseAccount::decode(&any.value[..])?;
    Ok(AccountInfo {
        account_number: base.account_number,
        sequence: base.sequence,
    })
}

fn signer_key(a: &Args) -> Result<SigningKey, Box<dyn std::error::Error>> {
    if let Some(mnemonic) = a.mnemonic.as_ref() {
        // BIP39 mnemonic → BIP32 m/44'/118'/0'/0/0 (Cosmos coin type 118).
        let m = bip39::Mnemonic::parse(mnemonic.trim())?;
        let seed = m.to_seed("");
        let path: cosmrs::bip32::DerivationPath = "m/44'/118'/0'/0/0".parse()?;
        return Ok(SigningKey::derive_from_path(seed, &path)?);
    }
    let hex_key = a
        .key_hex
        .as_ref()
        .ok_or("missing --key-hex/--mnemonic (or RELAYER_KEY_HEX/RELAYER_MNEMONIC env)")?;
    let bytes = hex::decode(hex_key.trim_start_matches("0x"))?;
    Ok(SigningKey::from_slice(&bytes)?)
}

fn sign_and_encode(
    key: &SigningKey,
    msg: cosmrs::Any,
    a: &Args,
    account: &AccountInfo,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let chain_id: ChainId = a.cp_chain_id.parse()?;
    let fee_coin = Coin {
        amount: a.fee_amount,
        denom: a.fee_denom.parse()?,
    };
    let body = tx::Body::new(vec![msg], "", 0u16);
    // NOTE: `a.sequence` is the IBC *packet* sequence (recv-packet/jc-ack), not
    // an account-sequence override — always sign with the queried sequence.
    let sequence = account.sequence;
    let info = SignerInfo::single_direct(Some(key.public_key()), sequence);
    let auth = info.auth_info(Fee::from_amount_and_gas(fee_coin, a.gas));
    let doc = SignDoc::new(&body, &auth, &chain_id, account.account_number)?;
    Ok(doc.sign(key)?.to_bytes()?)
}

async fn broadcast(grpc: &str, tx_bytes: Vec<u8>) -> Result<String, Box<dyn std::error::Error>> {
    // CometBFT txhash = sha256(tx_bytes); the JunoClaw BroadcastTxResponse may
    // omit it, so compute locally as a fallback — GetTx can then fetch it.
    let local_hash = hex::encode_upper(Sha256::digest(&tx_bytes));
    let mut client = TxServiceClient::new(connect(grpc).await?)
        .max_decoding_message_size(32 * 1024 * 1024)
        .max_encoding_message_size(32 * 1024 * 1024);
    let resp = client
        .broadcast_tx(BroadcastTxRequest {
            tx_bytes,
            mode: BroadcastMode::Sync as i32,
        })
        .await?
        .into_inner();
    let tx_resp = resp.tx_response.ok_or("empty broadcast response")?;
    if tx_resp.code != 0 {
        return Err(format!(
            "tx failed (code {}): {}",
            tx_resp.code, tx_resp.raw_log
        )
        .into());
    }
    Ok(if tx_resp.txhash.is_empty() {
        local_hash
    } else {
        tx_resp.txhash
    })
}

/// Poll `GetTx` until the tx is committed. `BroadcastMode::Sync` only waits for
/// CheckTx, so a dependent message (e.g. an ibc-go proof that needs the
/// consensus state an update-client just landed) must wait for real inclusion.
async fn wait_tx(grpc: &str, txhash: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = TxServiceClient::new(connect(grpc).await?)
        .max_decoding_message_size(32 * 1024 * 1024)
        .max_encoding_message_size(32 * 1024 * 1024);
    for _ in 0..300 {
        match client
            .get_tx(GetTxRequest {
                hash: txhash.to_string(),
            })
            .await
        {
            Ok(resp) => {
                if let Some(tr) = resp.into_inner().tx_response {
                    if tr.code == 0 {
                        return Ok(());
                    }
                    return Err(
                        format!("tx {txhash} failed (code {}): {}", tr.code, tr.raw_log).into(),
                    );
                }
                return Ok(());
            }
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
        }
    }
    Err(format!("tx {txhash} not committed after polling").into())
}

/// Submit `MsgUpdateClient` for `client_id` anchored at `height`, then wait for
/// it to commit. Used by the standalone `update-client` subcommand and internally
/// by the proof-carrying handshake/packet commands, which must land a consensus
/// state at exactly `proof_height` before the ibc-go message verifies.
async fn update_client_to(
    a: &Args,
    client_id: &str,
    height: u64,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut client = layer_client(&a.layer_grpc).await?;
    let block = finalized_block(&mut client, height).await?;
    let header = ContractHeader {
        height: ContractHeight {
            revision_number: HEIGHT_REVISION_NUMBER,
            revision_height: height,
        },
        timestamp: block.timestamp_nanos,
        proposal_bytes: block.proposal_bytes,
        certificate_bytes: block.certificate_bytes,
    };
    let wasm_msg = WasmClientMessage {
        data: serde_json::to_vec(&header)?,
    };
    let key = signer_key(a)?;
    let signer = key.public_key().account_id(&a.bech32_prefix)?.to_string();
    let msg = MsgUpdateClient {
        client_id: client_id.to_string(),
        client_message: Some(proto_any_of(TYPE_URL_WASM_CLIENT_MESSAGE, &wasm_msg)),
        signer: signer.clone(),
    };
    let account = query_account(&a.grpc, &signer).await?;
    let tx_bytes = sign_and_encode(&key, any_of(TYPE_URL_MSG_UPDATE_CLIENT, &msg), a, &account)?;
    let hash = broadcast(&a.grpc, tx_bytes).await?;
    wait_tx(&a.grpc, &hash).await?;
    Ok(hash)
}

/// Dry-run the tx via the gRPC Simulate endpoint — returns the exact gas_used
/// the chain would charge, without consuming a sequence number or paying a fee.
async fn simulate(grpc: &str, tx_bytes: Vec<u8>) -> Result<u64, Box<dyn std::error::Error>> {
    let mut client = TxServiceClient::new(connect(grpc).await?)
        .max_decoding_message_size(32 * 1024 * 1024)
        .max_encoding_message_size(32 * 1024 * 1024);
    let resp = client
        .simulate(SimulateRequest {
            tx_bytes,
            ..Default::default()
        })
        .await?
        .into_inner();
    let used = resp
        .gas_info
        .map(|g| g.gas_used)
        .ok_or("simulate returned no gas_info")?;
    Ok(used)
}

/// Simulate and print the full result — gas plus the wasm events/log the
/// contract produced. Used to check MAYO verification output without
/// committing state.
async fn simulate_report(
    grpc: &str,
    tx_bytes: Vec<u8>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = TxServiceClient::new(connect(grpc).await?)
        .max_decoding_message_size(32 * 1024 * 1024)
        .max_encoding_message_size(32 * 1024 * 1024);
    let resp = client
        .simulate(SimulateRequest {
            tx_bytes,
            ..Default::default()
        })
        .await?
        .into_inner();
    if let Some(g) = &resp.gas_info {
        println!("simulate: gas_used={} gas_wanted={}", g.gas_used, g.gas_wanted);
    }
    if let Some(r) = &resp.result {
        if !r.log.is_empty() {
            println!("simulate: log {}", r.log);
        }
        for ev in &r.events {
            let attrs: Vec<String> = ev
                .attributes
                .iter()
                .map(|a| format!("{}={}", a.key, a.value))
                .collect();
            println!("  event {} [{}]", ev.r#type, attrs.join(" "));
        }
    }
    Ok(())
}

fn any_of<M: prost::Message>(type_url: &str, msg: &M) -> cosmrs::Any {
    cosmrs::Any {
        type_url: type_url.to_string(),
        value: msg.encode_to_vec(),
    }
}

fn proto_any_of<M: prost::Message>(type_url: &str, msg: &M) -> prost_types::Any {
    prost_types::Any {
        type_url: type_url.to_string(),
        value: msg.encode_to_vec(),
    }
}

/// Gov module account address for the counterparty chain — the authority
/// required by ibcwasm's MsgStoreCode. Derived as sha256("gov")[:20]
/// bech32-encoded with the chain prefix (same bytes on every SDK chain).
fn gov_authority(prefix: &str) -> Result<String, Box<dyn std::error::Error>> {
    let digest = Sha256::digest(b"gov");
    Ok(cosmrs::AccountId::new(prefix, &digest[..20])?.to_string())
}

// ---------------------------------------------------------------------------
// Subcommands
// ---------------------------------------------------------------------------

/// Resolve the target height: explicit --height N wins; 0 or omitted resolves
/// to the last FINALIZED tip. `LatestHeight` counts the proposed tip whose
/// block record (proposal_bytes/certificate_bytes) does not exist yet, so
/// anchoring a client or fetch at `tip` fails with "no proposal stored".
async fn resolve_height(
    client: &mut LightClientQueryClient<Channel>,
    requested: Option<u64>,
) -> Result<u64, Box<dyn std::error::Error>> {
    match requested {
        Some(h) if h > 0 => Ok(h),
        _ => Ok(client
            .latest_height(QueryLatestHeightRequest {})
            .await?
            .into_inner()
            .height
            .saturating_sub(1)),
    }
}

/// Fetch `block(height)`, polling until the proposal finalizes — block records
/// (proposal_bytes/certificate_bytes) only exist once the Reporter stores the
/// finalization certificate, so querying the tip immediately returns NotFound.
async fn finalized_block(
    client: &mut LightClientQueryClient<Channel>,
    height: u64,
) -> Result<QueryBlockResponse, Box<dyn std::error::Error>> {
    let mut attempts = 0u32;
    loop {
        match client.block(QueryBlockRequest { height }).await {
            Ok(b) => return Ok(b.into_inner()),
            Err(e) => {
                attempts += 1;
                if attempts >= 200 {
                    return Err(format!(
                        "block {height} not finalized after {attempts} tries: {e}"
                    )
                    .into());
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
}

async fn cmd_fetch(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = layer_client(&a.layer_grpc).await?;
    let height = resolve_height(&mut client, a.height).await?;
    let resp = finalized_block(&mut client, height).await?;

    let proposal = decode_proposal(&resp.proposal_bytes)?;
    println!("height:            {}", resp.height);
    println!("timestamp_nanos:   {}", resp.timestamp_nanos);
    println!("epoch/view/parent: {}/{}/{}", proposal.epoch, proposal.view, proposal.parent);
    println!("payload_digest:    {}", hex::encode(proposal.payload));
    println!("proposal_bytes:    {} bytes (b64 {})", resp.proposal_bytes.len(), base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &resp.proposal_bytes));
    println!("certificate_bytes: {} bytes (b64 {})", resp.certificate_bytes.len(), base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &resp.certificate_bytes));
    println!("payload_bytes:     {} bytes", resp.payload_bytes.len());
    Ok(())
}

async fn cmd_create_client(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let wasm_path = a.wasm.as_ref().ok_or("create-client requires --wasm")?;
    let group_pk = a
        .group_pubkey_hex
        .as_ref()
        .ok_or("create-client requires --group-pubkey-hex")?;

    // 1. Fetch the anchor block from the JunoClaw node.
    let mut client = layer_client(&a.layer_grpc).await?;
    let height = resolve_height(&mut client, a.height).await?;
    let block = finalized_block(&mut client, height).await?;
    let proposal = decode_proposal(&block.proposal_bytes)?;

    // 2. Contract JSON states.
    let client_state = ContractClientState {
        chain_id: a.chain_id.clone(),
        group_public_key_hex: group_pk.clone(),
        latest_height: ContractHeight {
            revision_number: HEIGHT_REVISION_NUMBER,
            revision_height: height,
        },
        frozen_height: None,
    };
    let consensus_state = ContractConsensusState {
        payload_digest_hex: hex::encode(proposal.payload),
        timestamp: block.timestamp_nanos,
        epoch: proposal.epoch,
        view: proposal.view,
        parent: proposal.parent,
    };

    // 3. Wrap in the 08-wasm protos.
    let checksum = Sha256::digest(std::fs::read(wasm_path)?).to_vec();
    let wasm_cs = WasmClientState {
        data: serde_json::to_vec(&client_state)?,
        checksum,
        latest_height: Some(IbcHeight {
            revision_number: HEIGHT_REVISION_NUMBER,
            revision_height: height,
        }),
    };
    let wasm_cons = WasmConsensusState {
        data: serde_json::to_vec(&consensus_state)?,
    };

    // 4. MsgCreateClient → sign → broadcast.
    let key = signer_key(a)?;
    let signer = key.public_key().account_id(&a.bech32_prefix)?.to_string();
    let msg = MsgCreateClient {
        client_state: Some(proto_any_of(TYPE_URL_WASM_CLIENT_STATE, &wasm_cs)),
        consensus_state: Some(proto_any_of(TYPE_URL_WASM_CONSENSUS_STATE, &wasm_cons)),
        signer: signer.clone(),
    };
    let account = query_account(&a.grpc, &signer).await?;
    println!("signer: {signer} (account_number {}, sequence {})", account.account_number, account.sequence);
    let tx_bytes = sign_and_encode(&key, any_of(TYPE_URL_MSG_CREATE_CLIENT, &msg), a, &account)?;
    let hash = broadcast(&a.grpc, tx_bytes).await?;
    println!("MsgCreateClient broadcast OK — txhash {hash}");
    Ok(())
}

async fn cmd_update_client(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let client_id = a
        .client_id
        .as_ref()
        .ok_or("update-client requires --client-id")?;
    let mut client = layer_client(&a.layer_grpc).await?;
    let height = resolve_height(&mut client, a.height).await?;
    let hash = update_client_to(a, client_id, height).await?;
    println!("MsgUpdateClient({client_id} @ {height}) broadcast OK — txhash {hash}");
    Ok(())
}

async fn cmd_store_code(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let wasm_path = a.wasm.as_ref().ok_or("store-code requires --wasm")?;
    let wasm_bytes = std::fs::read(wasm_path)?;
    println!("wasm: {} bytes, checksum {}", wasm_bytes.len(), hex::encode(Sha256::digest(&wasm_bytes)));

    let key = signer_key(a)?;
    let signer = key.public_key().account_id(&a.bech32_prefix)?.to_string();

    // --direct: broadcast cosmwasm MsgStoreCode straight (permissionless
    // chains like Juno). Default: wrap in a gov v1 proposal. --ibcwasm wraps
    // ibc.lightclients.wasm.v1.MsgStoreCode instead — the ibcwasm store is
    // authority-gated, so the inner signer is the gov module account.
    if a.direct {
        let store = MsgStoreCode {
            sender: signer.clone(),
            wasm_byte_code: wasm_bytes,
        };
        let account = query_account(&a.grpc, &signer).await?;
        println!("signer: {signer} (account_number {}, sequence {})", account.account_number, account.sequence);
        let tx_bytes = sign_and_encode(&key, any_of(TYPE_URL_MSG_STORE_CODE, &store), a, &account)?;
        let hash = broadcast(&a.grpc, tx_bytes).await?;
        println!("MsgStoreCode broadcast OK — txhash {hash}");
        return Ok(());
    }

    let inner = if a.ibcwasm {
        let authority = gov_authority(&a.bech32_prefix)?;
        println!("ibcwasm store via gov authority {authority}");
        proto_any_of(
            TYPE_URL_IBCWASM_MSG_STORE_CODE,
            &IbcWasmMsgStoreCode {
                signer: authority,
                wasm_byte_code: wasm_bytes,
            },
        )
    } else {
        proto_any_of(
            TYPE_URL_MSG_STORE_CODE,
            &MsgStoreCode {
                sender: signer.clone(),
                wasm_byte_code: wasm_bytes,
            },
        )
    };

    let deposit = a
        .deposit
        .clone()
        .unwrap_or_else(|| format!("1000000{}", a.fee_denom));
    let (amount, denom) = deposit
        .find(|c: char| c.is_alphabetic())
        .map(|i| deposit.split_at(i))
        .ok_or("invalid --deposit (expected <amount><denom>)")?;
    let proposal = MsgSubmitProposal {
        messages: vec![inner],
        initial_deposit: vec![ProtoCoin {
            denom: denom.to_string(),
            amount: amount.to_string(),
        }],
        proposer: signer.clone(),
        metadata: String::new(),
        title: a.title.clone().unwrap_or_else(|| "Upload JunoClaw BLS light client".into()),
        summary: a
            .summary
            .clone()
            .unwrap_or_else(|| "Stores the 08-wasm BLS light client contract.".into()),
        expedited: false,
    };

    let account = query_account(&a.grpc, &signer).await?;
    let tx_bytes = sign_and_encode(
        &key,
        any_of(TYPE_URL_MSG_SUBMIT_PROPOSAL, &proposal),
        a,
        &account,
    )?;
    if a.simulate {
        let used = simulate(&a.grpc, tx_bytes).await?;
        println!("simulate: gas_used={used} (tx {} bytes)", std::fs::metadata(wasm_path)?.len());
        return Ok(());
    }
    let hash = broadcast(&a.grpc, tx_bytes).await?;
    println!("MsgSubmitProposal(MsgStoreCode) broadcast OK — txhash {hash}");
    Ok(())
}

async fn cmd_assemble_proof(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let key_hex = a
        .storage_key_hex
        .as_ref()
        .ok_or("assemble-proof requires --storage-key-hex")?;
    let key = hex::decode(key_hex.trim_start_matches("0x"))?;

    let mut client = layer_client(&a.layer_grpc).await?;

    // 1. Membership proof over the latest committed state.
    let proof = client
        .proof(QueryProofRequest { key: key.clone() })
        .await?
        .into_inner();
    let proof_height = proof.state_height + 1;

    // 2. The payload at state_height+1 carries the state_root this proof
    //    verifies against (app-hash semantics). It may still be finalizing —
    //    poll until the block record exists.
    let block = finalized_block(&mut client, proof_height).await?;
    if block.payload_bytes.is_empty() {
        return Err(format!(
            "node has no payload_bytes for height {proof_height} — cannot assemble proof"
        )
        .into());
    }

    // 3. Emit the contract-side MembershipProof JSON.
    let contract_proof = ContractMembershipProof {
        payload_bytes: block.payload_bytes,
        leaf_index: proof.leaf_index,
        siblings: proof
            .siblings
            .iter()
            .map(|s| if s.is_empty() { None } else { Some(s.clone()) })
            .collect(),
    };
    let out = serde_json::json!({
        "proof_height": proof_height,
        "state_height": proof.state_height,
        "key": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &proof.key),
        "value": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &proof.value),
        "proof": contract_proof,
    });
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

async fn cmd_send(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let to = a.to.as_ref().ok_or("send requires --to <address>")?;
    let amount = a.amount.as_ref().ok_or("send requires --amount <n><denom>")?;
    let split = amount
        .find(|c: char| c.is_alphabetic())
        .ok_or("invalid --amount (expected <amount><denom>)")?;
    let (amt, denom) = amount.split_at(split);

    let key = signer_key(a)?;
    let signer = key.public_key().account_id(&a.bech32_prefix)?.to_string();
    let msg = MsgSend {
        from_address: signer.clone(),
        to_address: to.clone(),
        amount: vec![ProtoCoin {
            denom: denom.to_string(),
            amount: amt.to_string(),
        }],
    };
    let account = query_account(&a.grpc, &signer).await?;
    println!("signer: {signer} (account_number {}, sequence {})", account.account_number, account.sequence);
    let tx_bytes = sign_and_encode(&key, any_of("/cosmos.bank.v1beta1.MsgSend", &msg), a, &account)?;
    let hash = broadcast(&a.grpc, tx_bytes).await?;
    println!("MsgSend broadcast OK — txhash {hash}");
    Ok(())
}

/// Generate a fresh secp256k1 key and print the hex + bech32 address.
/// Fund the printed address via the uni-7 faucet, then pass the hex to the
/// other subcommands via --key-hex or RELAYER_KEY_HEX.
fn cmd_keygen(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    use cosmrs::crypto::secp256k1::SigningKey;
    // 32 bytes of OS entropy → secp256k1 signing key.
    let mut secret = [0u8; 32];
    getrandom::getrandom(&mut secret)?;
    let key = SigningKey::from_slice(&secret)?;
    let addr = key.public_key().account_id(&a.bech32_prefix)?;
    println!("key_hex: {}", hex::encode(secret));
    println!("address: {addr}");
    println!("fund via the uni-7 faucet, then: export RELAYER_KEY_HEX=<key_hex>");
    Ok(())
}

/// Query the counterparty account (account_number + sequence) — also serves
/// as a connectivity check against the uni-7 gRPC endpoint.
async fn cmd_account(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let key = signer_key(a)?;
    let addr = key.public_key().account_id(&a.bech32_prefix)?.to_string();
    match query_account(&a.grpc, &addr).await {
        Ok(info) => println!(
            "{addr}: account_number={} sequence={}",
            info.account_number, info.sequence
        ),
        Err(e) => println!("{addr}: query failed — {e}"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// JunoClaw-side IBC (sovereign IbcMsg → /junoclaw.ibc.v1.Msg)
// ---------------------------------------------------------------------------

// JunoClaw signing constants — mirror tools/tx-sender.
const LAYER_CHAIN_ID: &str = "junoclaw-1";
const LAYER_FEE_DENOM: &str = "ujclaw";
const LAYER_ACCOUNT_NUMBER: u64 = 17;
const LAYER_GAS: u64 = 4_000_000;
const LAYER_BECH32: &str = "juno";

/// JunoClaw signer — the funded deployer account by default; --jc-key-hex
/// (or JUNOCLAW_KEY_HEX env) overrides for a different funded account.
fn layer_signer_key(a: &Args) -> Result<SigningKey, Box<dyn std::error::Error>> {
    if let Some(h) = a.jc_key_hex.as_ref() {
        let bytes = hex::decode(h.trim_start_matches("0x"))?;
        return Ok(SigningKey::from_slice(&bytes)?);
    }
    let seed = Sha256::digest(b"junoclaw-deployer-v1");
    Ok(SigningKey::from_slice(&seed[..32])?)
}

/// The JunoClaw sender as a `layer_std::AccountId` (bech32 "juno1…").
fn layer_sender(a: &Args) -> Result<layer_std::AccountId, Box<dyn std::error::Error>> {
    let key = layer_signer_key(a)?;
    let addr = key.public_key().account_id(LAYER_BECH32)?.to_string();
    Ok(layer_std::AccountId::parse_string(&addr)?)
}

fn sign_and_encode_layer(
    key: &SigningKey,
    msg: cosmrs::Any,
    account: &AccountInfo,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let chain_id: ChainId = LAYER_CHAIN_ID.parse()?;
    let fee_coin = Coin {
        amount: 5000u128,
        denom: LAYER_FEE_DENOM.parse()?,
    };
    let body = tx::Body::new(vec![msg], "", 0u16);
    let info = SignerInfo::single_direct(Some(key.public_key()), account.sequence);
    let auth = info.auth_info(Fee::from_amount_and_gas(fee_coin, LAYER_GAS));
    let doc = SignDoc::new(&body, &auth, &chain_id, account.account_number)?;
    Ok(doc.sign(key)?.to_bytes()?)
}

/// Broadcast a proto `Any` signed with the JunoClaw deployer/--jc-key-hex key.
/// The chain decodes standard wasmd `/cosmwasm.wasm.v1.*` Anys into WasmMsg
/// (packages/cosmos/src/msg.rs::parse_cosmos_msg), so contract lifecycle
/// messages go through this same path — no gov proposal required.
async fn submit_layer_any(a: &Args, any: cosmrs::Any) -> Result<String, Box<dyn std::error::Error>> {
    let key = layer_signer_key(a)?;
    let signer_addr = key.public_key().account_id(LAYER_BECH32)?.to_string();
    let mut account = query_account(&a.layer_grpc, &signer_addr).await?;
    if account.account_number == 0 {
        account.account_number = LAYER_ACCOUNT_NUMBER;
    }
    let tx_bytes = sign_and_encode_layer(&key, any, &account)?;
    if a.simulate {
        simulate_report(&a.layer_grpc, tx_bytes).await?;
        return Ok("simulated".into());
    }
    broadcast(&a.layer_grpc, tx_bytes).await
}

/// JSON-encode an `IbcMsg` into a single `Any` and broadcast it to JunoClaw.
async fn submit_ibc_msg(a: &Args, msg: IbcMsg) -> Result<String, Box<dyn std::error::Error>> {
    let any = cosmrs::Any {
        type_url: ibc::TYPE_URL_JUNOCLAW_IBC.to_string(),
        value: serde_json::to_vec(&msg)?,
    };
    submit_layer_any(a, any).await
}

/// `jc-store-code` — upload a CosmWasm contract to JunoClaw (permissionless).
async fn cmd_jc_store_code(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let wasm_path = a.wasm.as_ref().ok_or("jc-store-code requires --wasm")?;
    let wasm_bytes = std::fs::read(wasm_path)?;
    println!(
        "wasm: {} bytes, checksum {}",
        wasm_bytes.len(),
        hex::encode(Sha256::digest(&wasm_bytes))
    );
    let msg = MsgStoreCode {
        sender: layer_sender(a)?.to_string(),
        wasm_byte_code: wasm_bytes,
    };
    let hash = submit_layer_any(a, any_of(TYPE_URL_MSG_STORE_CODE, &msg)).await?;
    println!("jc-store-code broadcast OK — txhash {hash}");
    Ok(())
}

/// Resolve a `--msg` argument: literal JSON, or `@<path>` to read a file.
/// Files are needed for MAYO vectors — `Vec<u8>` fields serialize as
/// multi-KB JSON int arrays that don't fit comfortably on the command line.
fn load_msg_json(arg: &str) -> Result<String, Box<dyn std::error::Error>> {
    match arg.strip_prefix('@') {
        Some(path) => Ok(String::from_utf8(std::fs::read(path)?)?),
        None => Ok(arg.to_string()),
    }
}

/// `jc-instantiate` — instantiate a stored code id on JunoClaw.
async fn cmd_jc_instantiate(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let code_id = a.code_id.ok_or("jc-instantiate requires --code-id")?;
    let msg_json = match &a.msg_json {
        Some(m) => load_msg_json(m)?,
        None => "{}".into(),
    };
    let msg = MsgInstantiateContract {
        sender: layer_sender(a)?.to_string(),
        admin: String::new(),
        code_id,
        label: a
            .label
            .clone()
            .unwrap_or_else(|| format!("jc-code-{code_id}")),
        msg: msg_json.into_bytes(),
        funds: vec![],
    };
    let hash = submit_layer_any(a, any_of(TYPE_URL_MSG_INSTANTIATE_CONTRACT, &msg)).await?;
    println!("jc-instantiate code_id={code_id} broadcast OK — txhash {hash}");

    // Wait for the tx to land in a finalized block, then surface the new
    // contract address via ContractsByCode (addresses are generated
    // deterministically from (sender, code_id, counter) but this avoids
    // depending on the counter).
    let mut client = WasmQueryClient::new(connect(&a.layer_grpc).await?);
    for attempt in 0..40 {
        tokio::time::sleep(Duration::from_millis(750)).await;
        let resp = client
            .contracts_by_code(QueryContractsByCodeRequest {
                code_id,
                pagination: None,
            })
            .await;
        if let Ok(r) = resp {
            let contracts = r.into_inner().contracts;
            if !contracts.is_empty() {
                for c in &contracts {
                    println!("contract[{code_id}]: {c}");
                }
                return Ok(());
            }
        } else if attempt == 39 {
            eprintln!("warn: ContractsByCode query failed — look the contract up via jc-contracts");
        }
    }
    eprintln!("warn: no contract visible for code_id={code_id} after 30s");
    Ok(())
}

/// `jc-execute` — call an instantiated contract on JunoClaw.
async fn cmd_jc_execute(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let contract = a.contract.clone().ok_or("jc-execute requires --contract")?;
    let msg_json = load_msg_json(
        a.msg_json
            .as_deref()
            .ok_or("jc-execute requires --msg <json|@file>")?,
    )?;
    let msg = MsgExecuteContract {
        sender: layer_sender(a)?.to_string(),
        contract: contract.clone(),
        msg: msg_json.into_bytes(),
        funds: vec![],
    };
    let hash = submit_layer_any(a, any_of(TYPE_URL_MSG_EXECUTE_CONTRACT, &msg)).await?;
    println!("jc-execute {contract} broadcast OK — txhash {hash}");
    Ok(())
}

/// `jc-contracts` — list contract addresses instantiated from a code id.
async fn cmd_jc_contracts(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let code_id = a.code_id.ok_or("jc-contracts requires --code-id")?;
    let mut client = WasmQueryClient::new(connect(&a.layer_grpc).await?);
    let resp = client
        .contracts_by_code(QueryContractsByCodeRequest {
            code_id,
            pagination: None,
        })
        .await?
        .into_inner();
    for c in &resp.contracts {
        println!("{c}");
    }
    Ok(())
}

/// `jc-query` — smart-contract query on JunoClaw (abci_query path, no gas).
async fn cmd_jc_query(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let contract = a.contract.clone().ok_or("jc-query requires --contract")?;
    let msg_json = load_msg_json(
        a.msg_json
            .as_deref()
            .ok_or("jc-query requires --msg <json|@file>")?,
    )?;
    let mut client = WasmQueryClient::new(connect(&a.layer_grpc).await?);
    let resp = client
        .smart_contract_state(QuerySmartContractStateRequest {
            address: contract,
            query_data: msg_json.into_bytes(),
        })
        .await?
        .into_inner();
    let out = String::from_utf8_lossy(&resp.data);
    println!("{out}");
    Ok(())
}

/// Query JunoClaw for a Merkle membership proof over `key` and return the
/// contract-side `MembershipProof` JSON bytes plus the `proof_height`
/// (`state_height + 1`) the counterparty's 08-wasm client must already have a
/// consensus state for.
async fn assemble_membership_proof(
    layer_grpc: &str,
    key: Vec<u8>,
) -> Result<(Vec<u8>, u64), Box<dyn std::error::Error>> {
    let mut client = layer_client(layer_grpc).await?;
    let proof = client
        .proof(QueryProofRequest { key })
        .await?
        .into_inner();
    let proof_height = proof.state_height + 1;
    // Block `proof_height` carries the state_root for the proven state, but it
    // is the unfinalized tip at query time — poll until it commits.
    let block = finalized_block(&mut client, proof_height).await?;
    if block.payload_bytes.is_empty() {
        return Err(format!("node has no payload_bytes at height {proof_height}").into());
    }
    let contract_proof = ContractMembershipProof {
        payload_bytes: block.payload_bytes,
        leaf_index: proof.leaf_index,
        siblings: proof
            .siblings
            .iter()
            .map(|s| if s.is_empty() { None } else { Some(s.clone()) })
            .collect(),
    };
    Ok((serde_json::to_vec(&contract_proof)?, proof_height))
}

fn height(h: u64) -> Option<IbcHeight> {
    Some(IbcHeight {
        revision_number: HEIGHT_REVISION_NUMBER,
        revision_height: h,
    })
}

/// Height for fields JunoClaw encodes itself — the packet `timeout_height` it
/// hashes into the ICS-20 commitment. JunoClaw's keeper always uses revision 0
/// there, so this must NOT use `HEIGHT_REVISION_NUMBER` (that constant is only
/// for heights the 08-wasm *client* consumes: proof/consensus/latest). Using
/// rev=1 here would make ibc-go compute a different commitment than the one
/// JunoClaw stored. Note a non-zero rev=0 timeout_height reads as "already
/// timed out" against the rev=1 client/Osmosis height, so pass
/// `--timeout-height 0` and rely on `--timeout-timestamp`.
fn commitment_height(h: u64) -> Option<IbcHeight> {
    Some(IbcHeight {
        revision_number: 0,
        revision_height: h,
    })
}

// ---- JunoClaw IbcMsg subcommands ----

async fn cmd_jc_create_client(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let client_id = a.client_id.as_ref().ok_or("jc-create-client requires --client-id")?;
    let msg = IbcMsg::CreateClient {
        sender: layer_sender(a)?,
        client_id: client_id.clone(),
        chain_id: a.cp_chain_id.clone(),
        latest_height: a.height.unwrap_or(0),
    };
    let hash = submit_ibc_msg(a, msg).await?;
    println!("IbcMsg::CreateClient({client_id} -> {}) broadcast OK — txhash {hash}", a.cp_chain_id);
    Ok(())
}

async fn cmd_jc_conn_init(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let client_id = a.client_id.as_ref().ok_or("jc-conn-init requires --client-id")?;
    let connection_id = a
        .connection_id
        .as_ref()
        .ok_or("jc-conn-init requires --connection-id")?;
    let cp_client_id = a
        .cp_client_id
        .as_ref()
        .ok_or("jc-conn-init requires --cp-client-id")?;
    let msg = IbcMsg::ConnectionOpenInit {
        sender: layer_sender(a)?,
        client_id: client_id.clone(),
        connection_id: connection_id.clone(),
        counterparty_client_id: cp_client_id.clone(),
        counterparty_connection_id: a.cp_connection_id.clone().unwrap_or_default(),
        // The counterparty (Osmosis) commitment prefix stored in our
        // ConnectionEnd — ibc-go expects the standard "ibc" store prefix.
        counterparty_prefix: "ibc".to_string(),
    };
    let hash = submit_ibc_msg(a, msg).await?;
    println!("IbcMsg::ConnectionOpenInit({connection_id}) broadcast OK — txhash {hash}");
    Ok(())
}

async fn cmd_jc_conn_ack(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let connection_id = a
        .connection_id
        .as_ref()
        .ok_or("jc-conn-ack requires --connection-id")?;
    let cp_connection_id = a
        .cp_connection_id
        .as_ref()
        .ok_or("jc-conn-ack requires --cp-connection-id")?;
    let msg = IbcMsg::ConnectionOpenAck {
        sender: layer_sender(a)?,
        connection_id: connection_id.clone(),
        counterparty_connection_id: cp_connection_id.clone(),
        proof: cosmwasm_std::Binary::default(),
        proof_height: a.proof_height.unwrap_or(0),
    };
    let hash = submit_ibc_msg(a, msg).await?;
    println!("IbcMsg::ConnectionOpenAck({connection_id}) broadcast OK — txhash {hash}");
    Ok(())
}

async fn cmd_jc_chan_init(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let port_id = a.port_id.clone().unwrap_or_else(|| "transfer".into());
    let channel_id = a
        .channel_id
        .as_ref()
        .ok_or("jc-chan-init requires --channel-id")?;
    let connection_id = a
        .connection_id
        .as_ref()
        .ok_or("jc-chan-init requires --connection-id")?;
    let msg = IbcMsg::ChannelOpenInit {
        sender: layer_sender(a)?,
        port_id: port_id.clone(),
        channel_id: channel_id.clone(),
        connection_id: connection_id.clone(),
        counterparty_port_id: a.cp_port_id.clone().unwrap_or_else(|| "transfer".into()),
        counterparty_channel_id: a.cp_channel_id.clone().unwrap_or_default(),
        ordering: a.ordering.clone().unwrap_or_else(|| "UNORDERED".into()),
        version: a.version.clone().unwrap_or_else(|| "ics20-1".into()),
    };
    let hash = submit_ibc_msg(a, msg).await?;
    println!("IbcMsg::ChannelOpenInit({port_id}/{channel_id}) broadcast OK — txhash {hash}");
    Ok(())
}

async fn cmd_jc_chan_ack(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let port_id = a.port_id.clone().unwrap_or_else(|| "transfer".into());
    let channel_id = a
        .channel_id
        .as_ref()
        .ok_or("jc-chan-ack requires --channel-id")?;
    let cp_channel_id = a
        .cp_channel_id
        .as_ref()
        .ok_or("jc-chan-ack requires --cp-channel-id")?;
    let msg = IbcMsg::ChannelOpenAck {
        sender: layer_sender(a)?,
        port_id: port_id.clone(),
        channel_id: channel_id.clone(),
        counterparty_channel_id: cp_channel_id.clone(),
        counterparty_version: a.version.clone().unwrap_or_else(|| "ics20-1".into()),
        proof: cosmwasm_std::Binary::default(),
        proof_height: a.proof_height.unwrap_or(0),
    };
    let hash = submit_ibc_msg(a, msg).await?;
    println!("IbcMsg::ChannelOpenAck({port_id}/{channel_id}) broadcast OK — txhash {hash}");
    Ok(())
}

async fn cmd_jc_transfer(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let port_id = a.port_id.clone().unwrap_or_else(|| "transfer".into());
    let channel_id = a
        .channel_id
        .as_ref()
        .ok_or("jc-transfer requires --channel-id")?;
    let receiver = a.to.as_ref().ok_or("jc-transfer requires --to <receiver>")?;
    let amount = a
        .amount
        .as_ref()
        .ok_or("jc-transfer requires --amount <n><denom>")?;
    let split = amount
        .find(|c: char| c.is_alphabetic())
        .ok_or("invalid --amount (expected <amount><denom>)")?;
    let (amt, denom) = amount.split_at(split);
    let msg = IbcMsg::Transfer {
        sender: layer_sender(a)?,
        port_id: port_id.clone(),
        channel_id: channel_id.clone(),
        token: cosmwasm_std::Coin {
            denom: denom.to_string(),
            amount: cosmwasm_std::Uint128::from(amt.parse::<u128>()?),
        },
        receiver: receiver.clone(),
        timeout_height: a.timeout_height.unwrap_or(0),
        timeout_timestamp: a.timeout_timestamp.unwrap_or(0),
    };
    let hash = submit_ibc_msg(a, msg).await?;
    println!("IbcMsg::Transfer({amount} {port_id}/{channel_id} -> {receiver}) broadcast OK — txhash {hash}");
    Ok(())
}

async fn cmd_jc_ack(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let port_id = a.port_id.clone().unwrap_or_else(|| "transfer".into());
    let channel_id = a
        .channel_id
        .as_ref()
        .ok_or("jc-ack requires --channel-id")?;
    let sequence = a.sequence.ok_or("jc-ack requires --sequence")?;
    let msg = IbcMsg::Acknowledgement {
        sender: layer_sender(a)?,
        port_id: port_id.clone(),
        channel_id: channel_id.clone(),
        sequence,
        acknowledgement: cosmwasm_std::Binary::default(),
        proof: cosmwasm_std::Binary::default(),
        proof_height: a.proof_height.unwrap_or(0),
    };
    let hash = submit_ibc_msg(a, msg).await?;
    println!("IbcMsg::Acknowledgement({port_id}/{channel_id} seq {sequence}) broadcast OK — txhash {hash}");
    Ok(())
}

// ---- Counterparty ibc-go handshake / packet subcommands ----

async fn cmd_conn_try(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let client_id = a
        .client_id
        .as_ref()
        .ok_or("conn-try requires --client-id (the 08-wasm client)")?;
    let cp_client_id = a
        .cp_client_id
        .as_ref()
        .ok_or("conn-try requires --cp-client-id")?;
    let cp_connection_id = a
        .cp_connection_id
        .as_ref()
        .ok_or("conn-try requires --cp-connection-id")?;

    // Proof of JunoClaw's ConnectionEnd{INIT} at ibc/connections/<cp_conn>.
    let key = format!("ibc/connections/{cp_connection_id}").into_bytes();
    let (proof_init, proof_height) = assemble_membership_proof(&a.layer_grpc, key).await?;
    println!("proof_init over ibc/connections/{cp_connection_id} @ height {proof_height}");

    // The 08-wasm client needs a consensus state at exactly proof_height.
    let h = update_client_to(a, client_id, proof_height).await?;
    println!("MsgUpdateClient({client_id} @ {proof_height}) broadcast OK — txhash {h}");

    let key = signer_key(a)?;
    let signer = key.public_key().account_id(&a.bech32_prefix)?.to_string();
    let msg = ibc::MsgConnectionOpenTry {
        client_id: client_id.clone(),
        previous_connection_id: String::new(),
        client_state: None, // devnet: skip counterparty client/consensus proofs
        counterparty: Some(ibc::ConnectionCounterparty {
            client_id: cp_client_id.clone(),
            connection_id: cp_connection_id.clone(),
            prefix: Some(ibc::MerklePrefix {
                key_prefix: ibc::COUNTERPARTY_PREFIX.to_vec(),
            }),
        }),
        delay_period: 0,
        counterparty_versions: vec![ibc::Version {
            identifier: "1".into(),
            features: vec!["ORDER_ORDERED".into(), "ORDER_UNORDERED".into()],
        }],
        proof_height: height(proof_height),
        proof_init,
        proof_client: vec![],
        proof_consensus: vec![],
        consensus_height: height(proof_height),
        signer: signer.clone(),
    };
    let account = query_account(&a.grpc, &signer).await?;
    let tx_bytes = sign_and_encode(&key, any_of(ibc::TYPE_URL_CONN_OPEN_TRY, &msg), a, &account)?;
    let hash = broadcast(&a.grpc, tx_bytes).await?;
    println!("MsgConnectionOpenTry broadcast OK — txhash {hash}");
    Ok(())
}

async fn cmd_conn_confirm(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let client_id = a
        .client_id
        .as_ref()
        .ok_or("conn-confirm requires --client-id (the 08-wasm client)")?;
    let connection_id = a
        .connection_id
        .as_ref()
        .ok_or("conn-confirm requires --connection-id")?;
    let cp_connection_id = a
        .cp_connection_id
        .as_ref()
        .ok_or("conn-confirm requires --cp-connection-id")?;

    // Proof of JunoClaw's ConnectionEnd{OPEN} at ibc/connections/<cp_conn>.
    let key = format!("ibc/connections/{cp_connection_id}").into_bytes();
    let (proof_ack, proof_height) = assemble_membership_proof(&a.layer_grpc, key).await?;
    println!("proof_ack over ibc/connections/{cp_connection_id} @ height {proof_height}");

    // The 08-wasm client needs a consensus state at exactly proof_height.
    let h = update_client_to(a, client_id, proof_height).await?;
    println!("MsgUpdateClient({client_id} @ {proof_height}) broadcast OK — txhash {h}");

    let key = signer_key(a)?;
    let signer = key.public_key().account_id(&a.bech32_prefix)?.to_string();
    let msg = ibc::MsgConnectionOpenConfirm {
        connection_id: connection_id.clone(),
        proof_ack,
        proof_height: height(proof_height),
        signer: signer.clone(),
    };
    let account = query_account(&a.grpc, &signer).await?;
    let tx_bytes = sign_and_encode(&key, any_of(ibc::TYPE_URL_CONN_OPEN_CONFIRM, &msg), a, &account)?;
    let hash = broadcast(&a.grpc, tx_bytes).await?;
    println!("MsgConnectionOpenConfirm broadcast OK — txhash {hash}");
    Ok(())
}

async fn cmd_chan_try(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let client_id = a
        .client_id
        .as_ref()
        .ok_or("chan-try requires --client-id (the 08-wasm client)")?;
    let port_id = a.port_id.clone().unwrap_or_else(|| "transfer".into());
    let connection_id = a
        .connection_id
        .as_ref()
        .ok_or("chan-try requires --connection-id")?;
    let cp_port_id = a.cp_port_id.clone().unwrap_or_else(|| "transfer".into());
    let cp_channel_id = a
        .cp_channel_id
        .as_ref()
        .ok_or("chan-try requires --cp-channel-id")?;
    let version = a.version.clone().unwrap_or_else(|| "ics20-1".into());

    // Proof of JunoClaw's Channel{INIT} at ibc/channelEnds/ports/<cp_port>/channels/<cp_chan>.
    let key = format!("ibc/channelEnds/ports/{cp_port_id}/channels/{cp_channel_id}").into_bytes();
    let (proof_init, proof_height) = assemble_membership_proof(&a.layer_grpc, key).await?;
    println!("proof_init over channel {cp_port_id}/{cp_channel_id} @ height {proof_height}");

    // The 08-wasm client needs a consensus state at exactly proof_height.
    let h = update_client_to(a, client_id, proof_height).await?;
    println!("MsgUpdateClient({client_id} @ {proof_height}) broadcast OK — txhash {h}");

    let key = signer_key(a)?;
    let signer = key.public_key().account_id(&a.bech32_prefix)?.to_string();
    let msg = ibc::MsgChannelOpenTry {
        port_id: port_id.clone(),
        previous_channel_id: String::new(),
        channel: Some(ibc::Channel {
            state: ibc::ChannelState::TryOpen as i32,
            ordering: ibc::ChannelOrder::Unordered as i32,
            counterparty: Some(ibc::ChannelCounterparty {
                port_id: cp_port_id.clone(),
                channel_id: cp_channel_id.clone(),
            }),
            connection_hops: vec![connection_id.clone()],
            version: version.clone(),
        }),
        counterparty_version: version,
        proof_init,
        proof_height: height(proof_height),
        signer: signer.clone(),
    };
    let account = query_account(&a.grpc, &signer).await?;
    let tx_bytes = sign_and_encode(&key, any_of(ibc::TYPE_URL_CHAN_OPEN_TRY, &msg), a, &account)?;
    let hash = broadcast(&a.grpc, tx_bytes).await?;
    println!("MsgChannelOpenTry broadcast OK — txhash {hash}");
    Ok(())
}

async fn cmd_chan_confirm(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let client_id = a
        .client_id
        .as_ref()
        .ok_or("chan-confirm requires --client-id (the 08-wasm client)")?;
    let port_id = a.port_id.clone().unwrap_or_else(|| "transfer".into());
    let channel_id = a
        .channel_id
        .as_ref()
        .ok_or("chan-confirm requires --channel-id")?;
    let cp_port_id = a.cp_port_id.clone().unwrap_or_else(|| "transfer".into());
    let cp_channel_id = a
        .cp_channel_id
        .as_ref()
        .ok_or("chan-confirm requires --cp-channel-id")?;

    // Proof of JunoClaw's Channel{OPEN} at ibc/channelEnds/ports/<cp_port>/channels/<cp_chan>.
    let key = format!("ibc/channelEnds/ports/{cp_port_id}/channels/{cp_channel_id}").into_bytes();
    let (proof_ack, proof_height) = assemble_membership_proof(&a.layer_grpc, key).await?;
    println!("proof_ack over channel {cp_port_id}/{cp_channel_id} @ height {proof_height}");

    // The 08-wasm client needs a consensus state at exactly proof_height.
    let h = update_client_to(a, client_id, proof_height).await?;
    println!("MsgUpdateClient({client_id} @ {proof_height}) broadcast OK — txhash {h}");

    let key = signer_key(a)?;
    let signer = key.public_key().account_id(&a.bech32_prefix)?.to_string();
    let msg = ibc::MsgChannelOpenConfirm {
        port_id: port_id.clone(),
        channel_id: channel_id.clone(),
        proof_ack,
        proof_height: height(proof_height),
        signer: signer.clone(),
    };
    let account = query_account(&a.grpc, &signer).await?;
    let tx_bytes = sign_and_encode(&key, any_of(ibc::TYPE_URL_CHAN_OPEN_CONFIRM, &msg), a, &account)?;
    let hash = broadcast(&a.grpc, tx_bytes).await?;
    println!("MsgChannelOpenConfirm broadcast OK — txhash {hash}");
    Ok(())
}

async fn cmd_recv_packet(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let client_id = a
        .client_id
        .as_ref()
        .ok_or("recv-packet requires --client-id (the 08-wasm client)")?;
    let port_id = a.port_id.clone().unwrap_or_else(|| "transfer".into());
    let channel_id = a
        .channel_id
        .as_ref()
        .ok_or("recv-packet requires --channel-id")?;
    let cp_port_id = a.cp_port_id.clone().unwrap_or_else(|| "transfer".into());
    let cp_channel_id = a
        .cp_channel_id
        .as_ref()
        .ok_or("recv-packet requires --cp-channel-id")?;
    let sequence = a.sequence.ok_or("recv-packet requires --sequence")?;
    let receiver = a.to.as_ref().ok_or("recv-packet requires --to <receiver>")?;
    let amount = a
        .amount
        .as_ref()
        .ok_or("recv-packet requires --amount <n><denom>")?;
    let split = amount
        .find(|c: char| c.is_alphabetic())
        .ok_or("invalid --amount (expected <amount><denom>)")?;
    let (amt, denom) = amount.split_at(split);

    // Reconstruct the exact ICS-20 packet data JunoClaw committed.
    let jc_sender = layer_sender(a)?.to_string();
    let packet_data = format!(
        "{{\"amount\":\"{}\",\"denom\":\"{}\",\"receiver\":\"{}\",\"sender\":\"{}\"}}",
        amt, denom, receiver, jc_sender
    )
    .into_bytes();

    // Proof of JunoClaw's packet commitment at
    // ibc/commitments/ports/<src_port>/channels/<src_chan>/sequences/<seq>.
    let key = format!(
        "ibc/commitments/ports/{cp_port_id}/channels/{cp_channel_id}/sequences/{sequence}"
    )
    .into_bytes();
    let (proof_commitment, proof_height) = assemble_membership_proof(&a.layer_grpc, key).await?;
    println!("proof_commitment over packet seq {sequence} @ height {proof_height}");

    // The 08-wasm client needs a consensus state at exactly proof_height.
    let h = update_client_to(a, client_id, proof_height).await?;
    println!("MsgUpdateClient({client_id} @ {proof_height}) broadcast OK — txhash {h}");

    let key = signer_key(a)?;
    let signer = key.public_key().account_id(&a.bech32_prefix)?.to_string();
    let msg = ibc::MsgRecvPacket {
        packet: Some(ibc::Packet {
            sequence,
            source_port: cp_port_id.clone(),
            source_channel: cp_channel_id.clone(),
            destination_port: port_id.clone(),
            destination_channel: channel_id.clone(),
            data: packet_data,
            timeout_height: commitment_height(a.timeout_height.unwrap_or(0)),
            timeout_timestamp: a.timeout_timestamp.unwrap_or(0),
        }),
        proof_commitment,
        proof_height: height(proof_height),
        signer: signer.clone(),
    };
    let account = query_account(&a.grpc, &signer).await?;
    let tx_bytes = sign_and_encode(&key, any_of(ibc::TYPE_URL_RECV_PACKET, &msg), a, &account)?;
    let hash = broadcast(&a.grpc, tx_bytes).await?;
    println!("MsgRecvPacket broadcast OK — txhash {hash}");
    Ok(())
}

// ---------------------------------------------------------------------------
// relay daemon — automatic update-client cadence + packet recv/ack loop
// ---------------------------------------------------------------------------

/// JunoClaw ICS-24 storage keys (mirror `packages/app/src/ibc/paths.rs`).
fn jc_next_seq_key(port: &str, chan: &str) -> Vec<u8> {
    format!("ibc/nextSequenceSend/ports/{port}/channels/{chan}").into_bytes()
}
fn jc_commitment_key(port: &str, chan: &str, seq: u64) -> Vec<u8> {
    format!("ibc/commitments/ports/{port}/channels/{chan}/sequences/{seq}").into_bytes()
}
fn jc_packet_data_key(port: &str, chan: &str, seq: u64) -> Vec<u8> {
    format!("ibc/packetData/ports/{port}/channels/{chan}/sequences/{seq}").into_bytes()
}

/// Read a committed storage value via the lightclient `Proof` query — returns
/// `Some(value)` if the key exists, `None` if absent (non-membership proofs
/// aren't supported, so absence surfaces as `not_found`).
async fn query_value(
    layer_grpc: &str,
    key: Vec<u8>,
) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
    let mut client = layer_client(layer_grpc).await?;
    match client.proof(QueryProofRequest { key }).await {
        Ok(resp) => Ok(Some(resp.into_inner().value)),
        Err(e) if e.code() == tonic::Code::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Latest committed JunoClaw height.
async fn latest_height(layer_grpc: &str) -> Result<u64, Box<dyn std::error::Error>> {
    let mut client = layer_client(layer_grpc).await?;
    Ok(client
        .latest_height(QueryLatestHeightRequest {})
        .await?
        .into_inner()
        .height)
}

/// Does the counterparty hold a written acknowledgement for this packet? `true`
/// means it was received + acked on the dest chain, so JunoClaw's commitment is
/// safe to clear. `false` (including a query error) means "still needs recv" —
/// a conservative default that self-heals on the next tick.
async fn cp_acked(grpc: &str, port: &str, chan: &str, seq: u64) -> bool {
    let res: Result<bool, Box<dyn std::error::Error>> = async {
        let channel = connect(grpc).await?;
        let mut client: tonic::client::Grpc<Channel> = tonic::client::Grpc::new(channel);
        client.ready().await?;
        let path: http::uri::PathAndQuery =
            "/ibc.core.channel.v1.Query/PacketAcknowledgement".parse()?;
        let codec: ProstCodec<
            ibc::QueryPacketAcknowledgementRequest,
            ibc::QueryPacketAcknowledgementResponse,
        > = ProstCodec::default();
        let resp = client
            .unary(
                Request::new(ibc::QueryPacketAcknowledgementRequest {
                    port_id: port.to_string(),
                    channel_id: chan.to_string(),
                    sequence: seq,
                }),
                path,
                codec,
            )
            .await?;
        Ok(!resp.into_inner().acknowledgement.is_empty())
    }
    .await;
    res.unwrap_or(false)
}

/// Counterparty tip: (block height, block time in unix-nanos). Used to detect
/// elapsed packet timeouts — ibc-go rejects recv on an expired packet, so the
/// daemon must catch that case and submit IbcMsg::Timeout on JunoClaw instead.
async fn cp_latest(grpc: &str) -> Result<(u64, u64), Box<dyn std::error::Error>> {
    let mut client = TmServiceClient::new(connect(grpc).await?);
    let resp = client
        .get_latest_block(GetLatestBlockRequest {})
        .await?
        .into_inner();
    // SDK >=0.47 fills sdk_block (bech32 header); older chains fill the
    // deprecated tendermint block — same height/time fields either way.
    let header = resp
        .sdk_block
        .and_then(|b| b.header.map(|h| (h.height, h.time)))
        .or_else(|| resp.block.and_then(|b| b.header.map(|h| (h.height, h.time))));
    let (height, time) = header.ok_or("GetLatestBlock returned no header")?;
    let nanos = time
        .map(|t| t.seconds.max(0) as u64 * 1_000_000_000 + t.nanos.max(0) as u64)
        .unwrap_or(0);
    Ok((height.max(0) as u64, nanos))
}

/// Fee-token balance for `address` on `grpc` (cosmos.bank.v1beta1.Query).
/// Works on both the counterparty and JunoClaw if its gRPC exposes the same
/// service — callers treat a query error as "unknown", not "empty".
async fn bank_balance(
    grpc: &str,
    address: &str,
    denom: &str,
) -> Result<u128, Box<dyn std::error::Error>> {
    let mut client = BankQueryClient::new(connect(grpc).await?);
    let resp = client
        .balance(QueryBalanceRequest {
            address: address.to_string(),
            denom: denom.to_string(),
        })
        .await?
        .into_inner();
    let amount = resp.balance.map(|c| c.amount).unwrap_or_default();
    Ok(amount.parse().unwrap_or(0))
}

/// True if the packet's timeout has already elapsed on the counterparty.
/// `cp_height`/`cp_time_nanos` of 0 mean "no data" — a packet is never timed
/// out on missing information (conservative: keep trying recv).
fn packet_timed_out(packet: &ibc::Packet, cp_height: u64, cp_time_nanos: u64) -> bool {
    // Height timeout applies to the counterparty's revision height.
    let height_elapsed = packet
        .timeout_height
        .as_ref()
        .map(|h| h.revision_height != 0 && cp_height >= h.revision_height)
        .unwrap_or(false);
    let time_elapsed = packet.timeout_timestamp != 0 && cp_time_nanos >= packet.timeout_timestamp;
    height_elapsed || time_elapsed
}

/// Deliver one stored packet to the counterparty: prove JunoClaw's commitment,
/// advance the 08-wasm client to the proof height, then `MsgRecvPacket`.
async fn relay_recv_packet(
    a: &Args,
    client_id: &str,
    jc_port: &str,
    jc_channel: &str,
    packet: ibc::Packet,
) -> Result<(), Box<dyn std::error::Error>> {
    let seq = packet.sequence;
    let (proof_commitment, proof_height) =
        assemble_membership_proof(&a.layer_grpc, jc_commitment_key(jc_port, jc_channel, seq))
            .await?;
    let h = update_client_to(a, client_id, proof_height).await?;
    println!("relay: update-client {client_id}@{proof_height} txhash {h}");

    let key = signer_key(a)?;
    let signer = key.public_key().account_id(&a.bech32_prefix)?.to_string();
    let msg = ibc::MsgRecvPacket {
        packet: Some(packet),
        proof_commitment,
        proof_height: height(proof_height),
        signer: signer.clone(),
    };
    let account = query_account(&a.grpc, &signer).await?;
    let tx_bytes = sign_and_encode(&key, any_of(ibc::TYPE_URL_RECV_PACKET, &msg), a, &account)?;
    let hash = broadcast(&a.grpc, tx_bytes).await?;
    wait_tx(&a.grpc, &hash).await?;
    println!("relay: recv-packet seq {seq} txhash {hash}");
    Ok(())
}

/// Clear JunoClaw's packet commitment once the counterparty has acked.
async fn relay_ack(
    a: &Args,
    jc_port: &str,
    jc_channel: &str,
    seq: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let msg = IbcMsg::Acknowledgement {
        sender: layer_sender(a)?,
        port_id: jc_port.to_string(),
        channel_id: jc_channel.to_string(),
        sequence: seq,
        acknowledgement: cosmwasm_std::Binary::default(),
        proof: cosmwasm_std::Binary::default(),
        proof_height: 0,
    };
    let hash = submit_ibc_msg(a, msg).await?;
    println!("relay: jc-ack seq {seq} txhash {hash}");
    Ok(())
}

/// Refund a timed-out packet on JunoClaw: `IbcMsg::Timeout` refunds the escrow
/// to the original sender and clears commitment + stored packet.
async fn relay_timeout(
    a: &Args,
    jc_port: &str,
    jc_channel: &str,
    seq: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let msg = IbcMsg::Timeout {
        sender: layer_sender(a)?,
        port_id: jc_port.to_string(),
        channel_id: jc_channel.to_string(),
        sequence: seq,
        proof: cosmwasm_std::Binary::default(),
        proof_height: 0,
    };
    let hash = submit_ibc_msg(a, msg).await?;
    println!("relay: jc-timeout seq {seq} txhash {hash}");
    Ok(())
}

/// Daemon liveness + packet counters, shared with the optional health endpoint.
struct RelayStats {
    started: Instant,
    ticks: u64,
    errors: u64,
    panics: u64,
    recv: u64,
    acked: u64,
    timeouts: u64,
    pending: u32,
    jc_height: u64,
    cp_height: u64,
    cp_fee_balance: Option<u128>,
    last_tick_unix: u64,
    last_error: String,
}

impl RelayStats {
    fn to_json(&self) -> String {
        serde_json::json!({
            "uptime_secs": self.started.elapsed().as_secs(),
            "ticks": self.ticks,
            "errors": self.errors,
            "panics": self.panics,
            "packets_recv": self.recv,
            "packets_acked": self.acked,
            "packets_timed_out": self.timeouts,
            "packets_pending": self.pending,
            "junoclaw_height": self.jc_height,
            "cp_height": self.cp_height,
            "cp_fee_balance": self.cp_fee_balance,
            "last_tick_unix": self.last_tick_unix,
            "last_error": self.last_error,
        })
        .to_string()
    }
}

/// Daemon loop state carried across ticks.
struct RelayState {
    /// Last JunoClaw height pushed to the 08-wasm client.
    last_client_height: u64,
    /// Lowest sequence that may still hold a commitment — everything below it
    /// resolved, so earlier sequences are never re-queried (O(1) scans after
    /// steady state instead of O(nextSequenceSend)).
    scan_floor: u64,
    /// Per-sequence recv attempt count for --max-retries.
    retries: HashMap<u64, u32>,
}

/// Minimal HTTP health endpoint — every request returns the daemon stats as
/// JSON (200 OK). Alerting systems should watch `last_tick_unix` staleness,
/// `errors`/`panics` growth, and `packets_pending` accumulation. Runs for the
/// life of the daemon; a bind failure only warns and disables the endpoint.
async fn serve_health(addr: String, stats: Arc<Mutex<RelayStats>>) {
    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("relay: health endpoint bind {addr} failed: {e}");
            return;
        }
    };
    loop {
        let (mut sock, _) = match listener.accept().await {
            Ok(c) => c,
            Err(_) => continue,
        };
        let stats = stats.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf).await;
            let body = stats
                .lock()
                .map(|s| s.to_json())
                .unwrap_or_else(|_| "{}".into());
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = sock.write_all(resp.as_bytes()).await;
        });
    }
}

/// One daemon iteration: fee preflight → relay pending packets (ack → timeout
/// → recv, in that order) → keepalive-update the 08-wasm client.
async fn relay_tick(
    a: &Args,
    client_id: &str,
    jc_port: &str,
    jc_channel: &str,
    cp_port: &str,
    cp_channel: &str,
    state: &mut RelayState,
    stats: &Arc<Mutex<RelayStats>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // 0. Fee preflight — pause the tick rather than burn transactions that are
    //    guaranteed to fail BroadcastTx. --min-fee-balance 0 = auto (one tx fee).
    let cp_min_fee = if a.min_fee_balance == 0 {
        a.fee_amount
    } else {
        a.min_fee_balance
    };
    let cp_addr = signer_key(a)?
        .public_key()
        .account_id(&a.bech32_prefix)?
        .to_string();
    match bank_balance(&a.grpc, &cp_addr, &a.fee_denom).await {
        Ok(bal) => {
            if let Ok(mut s) = stats.lock() {
                s.cp_fee_balance = Some(bal);
            }
            if bal < cp_min_fee {
                eprintln!(
                    "relay: PAUSED — counterparty fee balance {bal}{} < {cp_min_fee} (top up {cp_addr})",
                    a.fee_denom
                );
                return Ok(());
            }
        }
        Err(e) => eprintln!("relay: counterparty balance query failed ({e}) — continuing"),
    }
    // Same check on the JunoClaw side — best effort: if the node's gRPC does
    // not expose cosmos.bank.v1beta1 the query errors and we only warn.
    let jc_addr = layer_sender(a)?.to_string();
    match bank_balance(&a.layer_grpc, &jc_addr, LAYER_FEE_DENOM).await {
        Ok(bal) if bal < 5000 => eprintln!(
            "relay: WARNING — JunoClaw fee balance {bal}{LAYER_FEE_DENOM} low ({jc_addr})"
        ),
        Ok(_) | Err(_) => {}
    }

    // 1. Relay any pending outbound packets on the JunoClaw channel.
    let next = match query_value(&a.layer_grpc, jc_next_seq_key(jc_port, jc_channel)).await? {
        Some(v) if v.len() >= 8 => u64::from_be_bytes(v[..8].try_into().unwrap()),
        _ => 1,
    };

    // Counterparty tip once per tick — needed for timeout detection.
    let (cp_height, cp_time_nanos) = cp_latest(&a.grpc).await.unwrap_or((0, 0));
    if let Ok(mut s) = stats.lock() {
        s.jc_height = latest_height(&a.layer_grpc).await.unwrap_or(0);
        s.cp_height = cp_height;
    }

    let mut lowest_pending: Option<u64> = None;
    let mut pending: u32 = 0;
    for seq in state.scan_floor.max(1)..next {
        // Commitment still present => not yet resolved on JunoClaw.
        if query_value(&a.layer_grpc, jc_commitment_key(jc_port, jc_channel, seq))
            .await?
            .is_none()
        {
            continue;
        }
        lowest_pending = lowest_pending.or(Some(seq));

        // Already received + acked on the counterparty? Just clear locally.
        if cp_acked(&a.grpc, cp_port, cp_channel, seq).await {
            relay_ack(a, jc_port, jc_channel, seq).await?;
            if let Ok(mut s) = stats.lock() {
                s.acked += 1;
            }
            continue;
        }
        pending += 1;

        // Read the stored packet — needed for both recv and timeout detection.
        let packet = match query_value(&a.layer_grpc, jc_packet_data_key(jc_port, jc_channel, seq))
            .await?
        {
            Some(v) => ibc::Packet::decode(v.as_slice())?,
            None => {
                eprintln!("relay: seq {seq} has a commitment but no stored packet — skipping");
                continue;
            }
        };

        // Timeout already elapsed on the counterparty? Re-delivering would be
        // rejected by ibc-go — submit IbcMsg::Timeout on JunoClaw instead to
        // refund the escrow to the original sender.
        if packet_timed_out(&packet, cp_height, cp_time_nanos) {
            relay_timeout(a, jc_port, jc_channel, seq).await?;
            if let Ok(mut s) = stats.lock() {
                s.timeouts += 1;
            }
            continue;
        }

        // Retry cap — a packet that keeps failing should not burn gas forever
        // (it pins the scan floor, so the cap is also what keeps the floor from
        // starving later sequences; resolve manually via jc-ack or timeout).
        let attempts = state.retries.get(&seq).copied().unwrap_or(0);
        if a.max_retries > 0 && attempts >= a.max_retries {
            if attempts == a.max_retries {
                eprintln!(
                    "relay: seq {seq} exceeded --max-retries {} — skipping",
                    a.max_retries
                );
                state.retries.insert(seq, a.max_retries + 1);
            }
            continue;
        }

        match relay_recv_packet(a, client_id, jc_port, jc_channel, packet).await {
            Ok(()) => {
                state.retries.remove(&seq);
                if let Ok(mut s) = stats.lock() {
                    s.recv += 1;
                }
                relay_ack(a, jc_port, jc_channel, seq).await?;
                if let Ok(mut s) = stats.lock() {
                    s.acked += 1;
                }
                state.last_client_height = latest_height(&a.layer_grpc)
                    .await
                    .unwrap_or(state.last_client_height);
            }
            Err(e) => {
                let n = state.retries.entry(seq).or_insert(0);
                *n += 1;
                eprintln!("relay: recv-packet seq {seq} failed (attempt {n}): {e}");
            }
        }
    }

    // Advance the scan floor to the lowest still-pending sequence — resolved
    // sequences are never re-queried.
    state.scan_floor = lowest_pending.unwrap_or(next);
    if let Ok(mut s) = stats.lock() {
        s.pending = pending;
    }

    // 2. Keepalive: advance the 08-wasm client on cadence so it never goes
    // stale. Anchor at the last FINALIZED height (tip - 1): proposal_bytes and
    // certificate_bytes are only stored once a block finalizes, so block(tip)
    // returns "no proposal stored".
    let latest = latest_height(&a.layer_grpc).await?.saturating_sub(1);
    if latest >= state.last_client_height + a.update_cadence {
        let hash = update_client_to(a, client_id, latest).await?;
        state.last_client_height = latest;
        println!("relay: keepalive update-client {client_id}@{latest} txhash {hash}");
    }
    Ok(())
}

/// `relay` — run the relayer as a daemon. Watches the JunoClaw outbound channel
/// for committed packets and resolves each in order: clear if already acked on
/// the counterparty, refund if its timeout elapsed, otherwise deliver
/// (`MsgRecvPacket`) and clear (`jc-ack`). Keeps the 08-wasm client fresh.
///
/// Robustness: each tick runs inside catch_unwind (a panic counts but never
/// kills the loop), consecutive tick errors back off exponentially (cap 60s),
/// a heartbeat line logs every ~minute, and --health-addr serves stats JSON.
///
/// Flags (JunoClaw-centric: `--channel-id` is the JunoClaw source channel,
/// `--cp-channel-id` the counterparty dest channel):
///   --client-id <08-wasm-N>   client on the counterparty (required)
///   --channel-id <id>         JunoClaw source channel (required)
///   --cp-channel-id <id>      counterparty dest channel (required)
///   --port-id / --cp-port-id  ports (default "transfer")
///   --interval <secs>         tick period (default 6)
///   --update-cadence <blocks> keepalive update-client every N blocks (default 50)
///   --max-retries <n>         per-packet recv attempts before skipping (default
///                             60; 0 = unlimited)
///   --min-fee-balance <n>     pause relaying when the counterparty fee balance
///                             drops below n (default 0 = auto: one tx fee)
///   --health-addr <host:port> serve stats JSON (e.g. 127.0.0.1:8080)
async fn cmd_relay(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let client_id = a
        .client_id
        .as_ref()
        .ok_or("relay requires --client-id (the 08-wasm client on the counterparty)")?;
    let jc_port = a.port_id.clone().unwrap_or_else(|| "transfer".into());
    let jc_channel = a
        .channel_id
        .as_ref()
        .ok_or("relay requires --channel-id (the JunoClaw source channel)")?;
    let cp_port = a.cp_port_id.clone().unwrap_or_else(|| "transfer".into());
    let cp_channel = a
        .cp_channel_id
        .as_ref()
        .ok_or("relay requires --cp-channel-id (the counterparty dest channel)")?;
    let interval = Duration::from_secs(a.interval.max(1));

    println!(
        "relay: watching {jc_port}/{jc_channel} -> {cp_port}/{cp_channel}, client {client_id}, tick {}s, keepalive every {} blocks, retry cap {}",
        interval.as_secs(),
        a.update_cadence,
        a.max_retries
    );

    let stats = Arc::new(Mutex::new(RelayStats {
        started: Instant::now(),
        ticks: 0,
        errors: 0,
        panics: 0,
        recv: 0,
        acked: 0,
        timeouts: 0,
        pending: 0,
        jc_height: 0,
        cp_height: 0,
        cp_fee_balance: None,
        last_tick_unix: 0,
        last_error: String::new(),
    }));

    if let Some(addr) = &a.health_addr {
        println!("relay: health endpoint on http://{addr}/health");
        tokio::spawn(serve_health(addr.clone(), stats.clone()));
    }

    let mut state = RelayState {
        last_client_height: 0,
        scan_floor: 1,
        retries: HashMap::new(),
    };
    let mut consecutive_errors: u32 = 0;

    loop {
        // Panic isolation: a panic inside a tick must not kill the daemon —
        // count it and keep the loop alive (external restart policies only
        // help on process exit; a recovered tick loop is strictly better).
        let res = AssertUnwindSafe(relay_tick(
            a,
            client_id,
            &jc_port,
            &jc_channel,
            &cp_port,
            &cp_channel,
            &mut state,
            &stats,
        ))
        .catch_unwind()
        .await;

        match res {
            Ok(Ok(())) => {
                consecutive_errors = 0;
                if let Ok(mut s) = stats.lock() {
                    s.ticks += 1;
                    s.last_tick_unix = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                }
            }
            Ok(Err(e)) => {
                consecutive_errors += 1;
                eprintln!("relay tick: {e}");
                if let Ok(mut s) = stats.lock() {
                    s.errors += 1;
                    s.last_error = e.to_string();
                }
            }
            Err(_) => {
                consecutive_errors += 1;
                eprintln!("relay: tick panicked — continuing");
                if let Ok(mut s) = stats.lock() {
                    s.panics += 1;
                    s.last_error = "tick panicked".into();
                }
            }
        }

        // Heartbeat roughly once a minute — proves liveness even when idle.
        if let Ok(s) = stats.lock() {
            if s.ticks % 10 == 1 || consecutive_errors > 0 {
                println!(
                    "relay: heartbeat ticks={} pending={} recv={} acked={} timeouts={} errors={} panics={} jc_h={} cp_h={}",
                    s.ticks,
                    s.pending,
                    s.recv,
                    s.acked,
                    s.timeouts,
                    s.errors,
                    s.panics,
                    s.jc_height,
                    s.cp_height
                );
            }
        }

        // Exponential backoff on consecutive tick errors (cap 60s).
        let backoff = interval
            .saturating_mul(1u32 << consecutive_errors.min(4))
            .min(Duration::from_secs(60));
        tokio::time::sleep(backoff).await;
    }
}

// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}\n");
            print_usage();
            std::process::exit(2);
        }
    };
    match a.cmd.as_str() {
        "fetch" => cmd_fetch(&a).await,
        "create-client" => cmd_create_client(&a).await,
        "update-client" => cmd_update_client(&a).await,
        "store-code" => cmd_store_code(&a).await,
        "assemble-proof" => cmd_assemble_proof(&a).await,
        "send" => cmd_send(&a).await,
        "keygen" => cmd_keygen(&a),
        "account" => cmd_account(&a).await,
        // JunoClaw sovereign IbcMsg
        "jc-create-client" => cmd_jc_create_client(&a).await,
        "jc-conn-init" => cmd_jc_conn_init(&a).await,
        "jc-conn-ack" => cmd_jc_conn_ack(&a).await,
        "jc-chan-init" => cmd_jc_chan_init(&a).await,
        "jc-chan-ack" => cmd_jc_chan_ack(&a).await,
        "jc-transfer" => cmd_jc_transfer(&a).await,
        "jc-ack" => cmd_jc_ack(&a).await,
        "jc-store-code" => cmd_jc_store_code(&a).await,
        "jc-instantiate" => cmd_jc_instantiate(&a).await,
        "jc-execute" => cmd_jc_execute(&a).await,
        "jc-contracts" => cmd_jc_contracts(&a).await,
        "jc-query" => cmd_jc_query(&a).await,
        // Counterparty ibc-go handshake + packet relay
        "conn-try" => cmd_conn_try(&a).await,
        "conn-confirm" => cmd_conn_confirm(&a).await,
        "chan-try" => cmd_chan_try(&a).await,
        "chan-confirm" => cmd_chan_confirm(&a).await,
        "recv-packet" => cmd_recv_packet(&a).await,
        // Daemon: auto update-client cadence + packet recv/ack loop
        "relay" => cmd_relay(&a).await,
        other => {
            eprintln!("unknown subcommand: {other}\n");
            print_usage();
            std::process::exit(2);
        }
    }
}
