use cosmwasm_std::{Binary, Coin};
use std::fmt::{Display, Formatter};

use crate::account_id::AccountId;

/// Internal IBC message format for JunoClaw's minimal IBC module.
///
/// Design note (minimal surface): JunoClaw is the *commitment writer*. It stores
/// ibc-go-encoded `ConnectionEnd`/`ChannelEnd`/packet-commitment bytes at the
/// ICS-24 paths inside its `state_root`-committed KV store, so the counterparty's
/// 08-wasm BLS light client can `verify_membership` on them. For the devnet demo
/// JunoClaw does *not* run a counterparty (Tendermint) light client — the
/// handshake `Ack` steps advance our side INIT→OPEN on relayer instruction and
/// carry the counterparty proof opaquely without verifying it. The genuine
/// counterparty verifier is the production (A56) phase.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(::cosmwasm_schema::serde::Serialize, ::cosmwasm_schema::serde::Deserialize)]
#[serde(crate = "::cosmwasm_schema::serde", rename_all = "snake_case")]
pub enum IbcMsg {
    /// Register a nominal counterparty light client (devnet: stores id/chain/height,
    /// no header verification).
    CreateClient {
        sender: AccountId,
        client_id: String,
        chain_id: String,
        latest_height: u64,
    },
    /// Advance the counterparty client's stored consensus height (devnet: nominal).
    UpdateClient {
        sender: AccountId,
        client_id: String,
        height: u64,
    },
    /// ICS-3: initiate a connection. Writes `ConnectionEnd` in `INIT` at
    /// `connections/{connection_id}`.
    ConnectionOpenInit {
        sender: AccountId,
        client_id: String,
        connection_id: String,
        counterparty_client_id: String,
        /// May be empty if the counterparty connection id is not yet known.
        counterparty_connection_id: String,
        /// Counterparty commitment prefix, e.g. "ibc".
        counterparty_prefix: String,
    },
    /// ICS-3: acknowledge the counterparty's TRY — move our connection to `OPEN`.
    /// `proof` is the counterparty's connection proof (devnet: carried, not verified).
    ConnectionOpenAck {
        sender: AccountId,
        connection_id: String,
        counterparty_connection_id: String,
        proof: Binary,
        proof_height: u64,
    },
    /// ICS-4: initiate a channel. Writes `ChannelEnd` in `INIT` at
    /// `channelEnds/ports/{port_id}/channels/{channel_id}`.
    ChannelOpenInit {
        sender: AccountId,
        port_id: String,
        channel_id: String,
        connection_id: String,
        counterparty_port_id: String,
        /// May be empty if the counterparty channel id is not yet known.
        counterparty_channel_id: String,
        /// "ORDERED" or "UNORDERED".
        ordering: String,
        version: String,
    },
    /// ICS-4: acknowledge the counterparty's TRY — move our channel to `OPEN`.
    ChannelOpenAck {
        sender: AccountId,
        port_id: String,
        channel_id: String,
        counterparty_channel_id: String,
        counterparty_version: String,
        proof: Binary,
        proof_height: u64,
    },
    /// ICS-20: escrow `token` and write the outgoing packet commitment at
    /// `commitments/ports/{port_id}/channels/{channel_id}/sequences/{seq}`.
    Transfer {
        sender: AccountId,
        port_id: String,
        channel_id: String,
        token: Coin,
        receiver: String,
        timeout_height: u64,
        timeout_timestamp: u64,
    },
    /// ICS-25: process an acknowledgement for a packet we sent. Clears the
    /// packet commitment (devnet: ack proof carried, not verified).
    Acknowledgement {
        sender: AccountId,
        port_id: String,
        channel_id: String,
        sequence: u64,
        acknowledgement: Binary,
        proof: Binary,
        proof_height: u64,
    },
}

impl From<IbcMsg> for crate::Msg {
    fn from(value: IbcMsg) -> Self {
        crate::Msg::Ibc(value)
    }
}

impl Display for IbcMsg {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            IbcMsg::CreateClient { .. } => f.write_str("IbcMsg::CreateClient"),
            IbcMsg::UpdateClient { .. } => f.write_str("IbcMsg::UpdateClient"),
            IbcMsg::ConnectionOpenInit { .. } => f.write_str("IbcMsg::ConnectionOpenInit"),
            IbcMsg::ConnectionOpenAck { .. } => f.write_str("IbcMsg::ConnectionOpenAck"),
            IbcMsg::ChannelOpenInit { .. } => f.write_str("IbcMsg::ChannelOpenInit"),
            IbcMsg::ChannelOpenAck { .. } => f.write_str("IbcMsg::ChannelOpenAck"),
            IbcMsg::Transfer { .. } => f.write_str("IbcMsg::Transfer"),
            IbcMsg::Acknowledgement { .. } => f.write_str("IbcMsg::Acknowledgement"),
        }
    }
}

/// Response data returned by IBC message handlers.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(::cosmwasm_schema::serde::Serialize, ::cosmwasm_schema::serde::Deserialize)]
#[serde(crate = "::cosmwasm_schema::serde", rename_all = "snake_case")]
pub enum IbcMsgData {
    CreateClient { client_id: String },
    UpdateClient {},
    ConnectionOpenInit {},
    ConnectionOpenAck {},
    ChannelOpenInit {},
    ChannelOpenAck {},
    Transfer { sequence: u64 },
    Acknowledgement {},
}

impl From<IbcMsgData> for crate::MsgData {
    fn from(value: IbcMsgData) -> Self {
        crate::MsgData::Ibc(value)
    }
}
