//! IBC wire types + helpers for the JunoClaw ↔ counterparty ICS-20 flow.
//!
//! Two families of messages live here:
//!
//! 1. **JunoClaw side** — the sovereign `IbcMsg` (from `layer-std`), JSON-encoded
//!    into a single `Any` under `/junoclaw.ibc.v1.Msg`. JunoClaw is the
//!    *commitment writer*: it stores ibc-go-encoded `ConnectionEnd`/`Channel`/
//!    packet-commitment bytes at `ibc/<ics24_path>` keys in its Merkle state.
//!
//! 2. **Counterparty side** — real ibc-go messages (connection/channel handshake
//!    + `MsgRecvPacket`) hand-defined as prost structs, wire-compatible with
//!    ibc-go v8+. These carry Merkle membership proofs that the counterparty's
//!    08-wasm BLS light client verifies against JunoClaw's committed state.
//!
//! The counterparty `MerklePrefix` is set to `"ibc/"` so that
//! `concat(key_path) == "ibc/<ics24_path>"` matches JunoClaw's storage key.

use crate::IbcHeight;

/// The counterparty commitment prefix the relayer sets on the connection.
/// ibc-go prepends it to each ICS-24 path when building the proof `key_path`,
/// so JunoClaw stores every commitment under `ibc/<path>`.
pub const COUNTERPARTY_PREFIX: &[u8] = b"ibc/";

// ---------------------------------------------------------------------------
// ibc-go core wire types
// ---------------------------------------------------------------------------

/// ibc.core.commitment.v1.MerklePrefix
#[derive(Clone, PartialEq, prost::Message)]
pub struct MerklePrefix {
    #[prost(bytes = "vec", tag = "1")]
    pub key_prefix: Vec<u8>,
}

/// ibc.core.connection.v1.Counterparty
#[derive(Clone, PartialEq, prost::Message)]
pub struct ConnectionCounterparty {
    #[prost(string, tag = "1")]
    pub client_id: String,
    #[prost(string, tag = "2")]
    pub connection_id: String,
    #[prost(message, optional, tag = "3")]
    pub prefix: Option<MerklePrefix>,
}

/// ibc.core.connection.v1.Version
#[derive(Clone, PartialEq, prost::Message)]
pub struct Version {
    #[prost(string, tag = "1")]
    pub identifier: String,
    #[prost(string, repeated, tag = "2")]
    pub features: Vec<String>,
}

/// ibc.core.channel.v1.Counterparty
#[derive(Clone, PartialEq, prost::Message)]
pub struct ChannelCounterparty {
    #[prost(string, tag = "1")]
    pub port_id: String,
    #[prost(string, tag = "2")]
    pub channel_id: String,
}

/// ibc.core.channel.v1.Channel
#[derive(Clone, PartialEq, prost::Message)]
pub struct Channel {
    #[prost(enumeration = "ChannelState", tag = "1")]
    pub state: i32,
    #[prost(enumeration = "ChannelOrder", tag = "2")]
    pub ordering: i32,
    #[prost(message, optional, tag = "3")]
    pub counterparty: Option<ChannelCounterparty>,
    #[prost(string, repeated, tag = "4")]
    pub connection_hops: Vec<String>,
    #[prost(string, tag = "5")]
    pub version: String,
}

/// ibc.core.channel.v1.State
#[derive(Clone, Copy, Debug, PartialEq, Eq, prost::Enumeration)]
#[repr(i32)]
pub enum ChannelState {
    Uninitialized = 0,
    Init = 1,
    TryOpen = 2,
    Open = 3,
    Closed = 4,
}

/// ibc.core.channel.v1.Order
#[derive(Clone, Copy, Debug, PartialEq, Eq, prost::Enumeration)]
#[repr(i32)]
pub enum ChannelOrder {
    NoneUnspecified = 0,
    Unordered = 1,
    Ordered = 2,
}

/// ibc.core.channel.v1.Packet
#[derive(Clone, PartialEq, prost::Message)]
pub struct Packet {
    #[prost(uint64, tag = "1")]
    pub sequence: u64,
    #[prost(string, tag = "2")]
    pub source_port: String,
    #[prost(string, tag = "3")]
    pub source_channel: String,
    #[prost(string, tag = "4")]
    pub destination_port: String,
    #[prost(string, tag = "5")]
    pub destination_channel: String,
    #[prost(bytes = "vec", tag = "6")]
    pub data: Vec<u8>,
    #[prost(message, optional, tag = "7")]
    pub timeout_height: Option<IbcHeight>,
    #[prost(uint64, tag = "8")]
    pub timeout_timestamp: u64,
}

// ---------------------------------------------------------------------------
// ibc-go handshake / packet messages (counterparty side)
// ---------------------------------------------------------------------------

/// ibc.core.connection.v1.MsgConnectionOpenTry
#[derive(Clone, PartialEq, prost::Message)]
pub struct MsgConnectionOpenTry {
    #[prost(string, tag = "1")]
    pub client_id: String,
    #[prost(string, tag = "2")]
    pub previous_connection_id: String,
    #[prost(message, optional, tag = "3")]
    pub client_state: Option<prost_types::Any>,
    #[prost(message, optional, tag = "4")]
    pub counterparty: Option<ConnectionCounterparty>,
    #[prost(uint64, tag = "5")]
    pub delay_period: u64,
    #[prost(message, repeated, tag = "6")]
    pub counterparty_versions: Vec<Version>,
    #[prost(message, optional, tag = "7")]
    pub proof_height: Option<IbcHeight>,
    #[prost(bytes = "vec", tag = "8")]
    pub proof_init: Vec<u8>,
    #[prost(bytes = "vec", tag = "9")]
    pub proof_client: Vec<u8>,
    #[prost(bytes = "vec", tag = "10")]
    pub proof_consensus: Vec<u8>,
    #[prost(message, optional, tag = "11")]
    pub consensus_height: Option<IbcHeight>,
    #[prost(string, tag = "12")]
    pub signer: String,
}

/// ibc.core.connection.v1.MsgConnectionOpenConfirm
#[derive(Clone, PartialEq, prost::Message)]
pub struct MsgConnectionOpenConfirm {
    #[prost(string, tag = "1")]
    pub connection_id: String,
    #[prost(bytes = "vec", tag = "2")]
    pub proof_ack: Vec<u8>,
    #[prost(message, optional, tag = "3")]
    pub proof_height: Option<IbcHeight>,
    #[prost(string, tag = "4")]
    pub signer: String,
}

/// ibc.core.channel.v1.MsgChannelOpenTry
#[derive(Clone, PartialEq, prost::Message)]
pub struct MsgChannelOpenTry {
    #[prost(string, tag = "1")]
    pub port_id: String,
    #[prost(string, tag = "2")]
    pub previous_channel_id: String,
    #[prost(message, optional, tag = "3")]
    pub channel: Option<Channel>,
    #[prost(string, tag = "4")]
    pub counterparty_version: String,
    #[prost(bytes = "vec", tag = "5")]
    pub proof_init: Vec<u8>,
    #[prost(message, optional, tag = "6")]
    pub proof_height: Option<IbcHeight>,
    #[prost(string, tag = "7")]
    pub signer: String,
}

/// ibc.core.channel.v1.MsgChannelOpenConfirm
#[derive(Clone, PartialEq, prost::Message)]
pub struct MsgChannelOpenConfirm {
    #[prost(string, tag = "1")]
    pub port_id: String,
    #[prost(string, tag = "2")]
    pub channel_id: String,
    #[prost(bytes = "vec", tag = "3")]
    pub proof_ack: Vec<u8>,
    #[prost(message, optional, tag = "4")]
    pub proof_height: Option<IbcHeight>,
    #[prost(string, tag = "5")]
    pub signer: String,
}

/// ibc.core.channel.v1.MsgRecvPacket
#[derive(Clone, PartialEq, prost::Message)]
pub struct MsgRecvPacket {
    #[prost(message, optional, tag = "1")]
    pub packet: Option<Packet>,
    #[prost(bytes = "vec", tag = "2")]
    pub proof_commitment: Vec<u8>,
    #[prost(message, optional, tag = "3")]
    pub proof_height: Option<IbcHeight>,
    #[prost(string, tag = "4")]
    pub signer: String,
}

/// ibc.core.channel.v1.QueryPacketAcknowledgementRequest — queries whether the
/// counterparty has written an ack for a packet (i.e. it was received). Used by
/// the relay daemon to decide between "still needs recv" and "safe to clear".
#[derive(Clone, PartialEq, prost::Message)]
pub struct QueryPacketAcknowledgementRequest {
    #[prost(string, tag = "1")]
    pub port_id: String,
    #[prost(string, tag = "2")]
    pub channel_id: String,
    #[prost(uint64, tag = "3")]
    pub sequence: u64,
}

/// ibc.core.channel.v1.QueryPacketAcknowledgementResponse
#[derive(Clone, PartialEq, prost::Message)]
pub struct QueryPacketAcknowledgementResponse {
    #[prost(bytes = "vec", tag = "1")]
    pub acknowledgement: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub proof: Vec<u8>,
    #[prost(message, optional, tag = "3")]
    pub proof_height: Option<IbcHeight>,
}

// Type URLs (ibc-go v8+)
pub const TYPE_URL_CONN_OPEN_TRY: &str = "/ibc.core.connection.v1.MsgConnectionOpenTry";
pub const TYPE_URL_CONN_OPEN_CONFIRM: &str = "/ibc.core.connection.v1.MsgConnectionOpenConfirm";
pub const TYPE_URL_CHAN_OPEN_TRY: &str = "/ibc.core.channel.v1.MsgChannelOpenTry";
pub const TYPE_URL_CHAN_OPEN_CONFIRM: &str = "/ibc.core.channel.v1.MsgChannelOpenConfirm";
pub const TYPE_URL_RECV_PACKET: &str = "/ibc.core.channel.v1.MsgRecvPacket";

/// JunoClaw sovereign IBC message — the `Any` value is a JSON-encoded `IbcMsg`.
pub const TYPE_URL_JUNOCLAW_IBC: &str = "/junoclaw.ibc.v1.Msg";
