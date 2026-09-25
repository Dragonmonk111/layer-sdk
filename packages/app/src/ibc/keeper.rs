use cosmwasm_std::{ensure_eq, BlockInfo, Coin, Event};
use sha2::{Digest, Sha256};

use cosmos_sdk_proto::ibc::core::channel::v1::{
    Channel, Counterparty as ChannelCounterparty, Order, State as ChannelState,
};
use cosmos_sdk_proto::ibc::core::client::v1::Height;
use cosmos_sdk_proto::ibc::core::commitment::v1::MerklePrefix;
use cosmos_sdk_proto::ibc::core::connection::v1::{
    ConnectionEnd, Counterparty as ConnectionCounterparty, State as ConnectionState, Version,
};
use cosmos_sdk_proto::ibc::core::channel::v1::Packet;
use cosmos_sdk_proto::prost::Message;

use layer_std::api::MsgResponse;
use layer_std::{AccountId, GasMeter, IbcMsg, IbcMsgData};
use layer_storage::{ReadonlyStorage, Storage};

use crate::error::{PulsarError, PulsarResult};
use crate::ibc::{paths, IbcError};
use crate::sm::StateMachine;

/// Deterministic ICS-20 escrow account for a (port, channel) pair.
/// Mirrors ibc-go's `GetEscrowAddress` intent: a module account no user controls.
fn escrow_account(port_id: &str, channel_id: &str) -> AccountId {
    let mut h = Sha256::new();
    h.update(b"ics20-1");
    h.update(port_id.as_bytes());
    h.update(channel_id.as_bytes());
    let digest = h.finalize();
    AccountId::new(&digest[..20]).expect("20-byte escrow address")
}

/// The IBC keeper. Minimal-surface commitment writer: stores ibc-go-encoded
/// connection/channel/packet-commitment bytes at ICS-24 paths inside the
/// `state_root`-committed KV store so the counterparty's 08-wasm BLS light
/// client can `verify_membership` on them.
#[derive(Default, Debug, Clone)]
pub struct Ibc {}

impl Ibc {
    pub fn new() -> Self {
        Ibc {}
    }

    // ---- raw storage helpers (literal ICS-24 path keys, no module prefix) ----

    fn write_raw(
        &self,
        storage: &mut dyn Storage,
        meter: &GasMeter,
        key: &[u8],
        value: &[u8],
    ) -> PulsarResult<()> {
        storage.set(meter, key, value)?;
        Ok(())
    }

    fn read_raw(
        &self,
        storage: &dyn ReadonlyStorage,
        meter: &GasMeter,
        key: &[u8],
    ) -> PulsarResult<Option<Vec<u8>>> {
        Ok(storage.get(meter, key)?)
    }

    // ---- message dispatch ----

    pub fn process_msg(
        &self,
        storage: &mut dyn Storage,
        meter: &GasMeter,
        block: &BlockInfo,
        sm: &StateMachine,
        signer: &AccountId,
        msg: IbcMsg,
    ) -> PulsarResult<MsgResponse> {
        match msg {
            IbcMsg::CreateClient {
                sender,
                client_id,
                chain_id,
                latest_height,
            } => {
                ensure_eq!(signer, &sender, self.unauthorized(signer));
                self.create_client(storage, meter, &client_id, &chain_id, latest_height)?;
                let events = vec![Event::new("ibc_create_client")
                    .add_attribute("client_id", &client_id)
                    .add_attribute("chain_id", &chain_id)];
                Ok(MsgResponse::new(
                    events,
                    IbcMsgData::CreateClient { client_id },
                ))
            }
            IbcMsg::UpdateClient {
                sender,
                client_id,
                height,
            } => {
                ensure_eq!(signer, &sender, self.unauthorized(signer));
                self.update_client(storage, meter, &client_id, height)?;
                let events = vec![Event::new("ibc_update_client")
                    .add_attribute("client_id", &client_id)
                    .add_attribute("height", height.to_string())];
                Ok(MsgResponse::new(events, IbcMsgData::UpdateClient {}))
            }
            IbcMsg::ConnectionOpenInit {
                sender,
                client_id,
                connection_id,
                counterparty_client_id,
                counterparty_connection_id,
                counterparty_prefix,
            } => {
                ensure_eq!(signer, &sender, self.unauthorized(signer));
                self.conn_open_init(
                    storage,
                    meter,
                    &client_id,
                    &connection_id,
                    &counterparty_client_id,
                    &counterparty_connection_id,
                    &counterparty_prefix,
                )?;
                let events = vec![Event::new("ibc_connection_open_init")
                    .add_attribute("connection_id", &connection_id)];
                Ok(MsgResponse::new(
                    events,
                    IbcMsgData::ConnectionOpenInit {},
                ))
            }
            IbcMsg::ConnectionOpenAck {
                sender,
                connection_id,
                counterparty_connection_id,
                ..
            } => {
                ensure_eq!(signer, &sender, self.unauthorized(signer));
                self.conn_open_ack(storage, meter, &connection_id, &counterparty_connection_id)?;
                let events = vec![Event::new("ibc_connection_open_ack")
                    .add_attribute("connection_id", &connection_id)];
                Ok(MsgResponse::new(events, IbcMsgData::ConnectionOpenAck {}))
            }
            IbcMsg::ChannelOpenInit {
                sender,
                port_id,
                channel_id,
                connection_id,
                counterparty_port_id,
                counterparty_channel_id,
                ordering,
                version,
            } => {
                ensure_eq!(signer, &sender, self.unauthorized(signer));
                self.chan_open_init(
                    storage,
                    meter,
                    &port_id,
                    &channel_id,
                    &connection_id,
                    &counterparty_port_id,
                    &counterparty_channel_id,
                    &ordering,
                    &version,
                )?;
                let events = vec![Event::new("ibc_channel_open_init")
                    .add_attribute("port_id", &port_id)
                    .add_attribute("channel_id", &channel_id)];
                Ok(MsgResponse::new(events, IbcMsgData::ChannelOpenInit {}))
            }
            IbcMsg::ChannelOpenAck {
                sender,
                port_id,
                channel_id,
                counterparty_channel_id,
                counterparty_version,
                ..
            } => {
                ensure_eq!(signer, &sender, self.unauthorized(signer));
                self.chan_open_ack(
                    storage,
                    meter,
                    &port_id,
                    &channel_id,
                    &counterparty_channel_id,
                    &counterparty_version,
                )?;
                let events = vec![Event::new("ibc_channel_open_ack")
                    .add_attribute("port_id", &port_id)
                    .add_attribute("channel_id", &channel_id)];
                Ok(MsgResponse::new(events, IbcMsgData::ChannelOpenAck {}))
            }
            IbcMsg::Transfer {
                sender,
                port_id,
                channel_id,
                token,
                receiver,
                timeout_height,
                timeout_timestamp,
            } => {
                ensure_eq!(signer, &sender, self.unauthorized(signer));
                let (sequence, packet_data) = self.transfer(
                    storage,
                    meter,
                    block,
                    sm,
                    &sender,
                    &port_id,
                    &channel_id,
                    &token,
                    &receiver,
                    timeout_height,
                    timeout_timestamp,
                )?;
                let events = vec![Event::new("ibc_transfer")
                    .add_attribute("port_id", &port_id)
                    .add_attribute("channel_id", &channel_id)
                    .add_attribute("sequence", sequence.to_string())
                    .add_attribute(
                        "packet_data",
                        String::from_utf8_lossy(&packet_data).to_string(),
                    )];
                Ok(MsgResponse::new(events, IbcMsgData::Transfer { sequence }))
            }
            IbcMsg::Acknowledgement {
                sender,
                port_id,
                channel_id,
                sequence,
                ..
            } => {
                ensure_eq!(signer, &sender, self.unauthorized(signer));
                self.acknowledgement(storage, meter, &port_id, &channel_id, sequence)?;
                let events = vec![Event::new("ibc_acknowledgement")
                    .add_attribute("port_id", &port_id)
                    .add_attribute("channel_id", &channel_id)
                    .add_attribute("sequence", sequence.to_string())];
                Ok(MsgResponse::new(events, IbcMsgData::Acknowledgement {}))
            }
            IbcMsg::Timeout {
                sender,
                port_id,
                channel_id,
                sequence,
                ..
            } => {
                ensure_eq!(signer, &sender, self.unauthorized(signer));
                let (refund_to, coin) =
                    self.timeout(storage, meter, block, sm, &port_id, &channel_id, sequence)?;
                let events = vec![Event::new("ibc_timeout")
                    .add_attribute("port_id", &port_id)
                    .add_attribute("channel_id", &channel_id)
                    .add_attribute("sequence", sequence.to_string())
                    .add_attribute("refund_to", refund_to.to_string())
                    .add_attribute("refund", format!("{}{}", coin.amount, coin.denom))];
                Ok(MsgResponse::new(events, IbcMsgData::Timeout {}))
            }
        }
    }

    fn unauthorized(&self, signer: &AccountId) -> PulsarError {
        IbcError::Unauthorized {
            signer: signer.to_string(),
        }
        .into()
    }

    // ---- client registry (nominal for devnet) ----

    fn create_client(
        &self,
        storage: &mut dyn Storage,
        meter: &GasMeter,
        client_id: &str,
        chain_id: &str,
        latest_height: u64,
    ) -> PulsarResult<()> {
        let key = paths::client_state_path(client_id);
        if self.read_raw(storage, meter, &key)?.is_some() {
            return Err(IbcError::ClientExists(client_id.to_string()).into());
        }
        // Nominal client state: {chain_id, latest_height} as JSON. Osmosis does
        // not verify our client state — only our connection/channel/packet
        // commitments — so this is bookkeeping for the devnet demo.
        let state = format!(
            "{{\"chain_id\":\"{}\",\"latest_height\":{}}}",
            chain_id, latest_height
        );
        self.write_raw(storage, meter, &key, state.as_bytes())
    }

    fn update_client(
        &self,
        storage: &mut dyn Storage,
        meter: &GasMeter,
        client_id: &str,
        height: u64,
    ) -> PulsarResult<()> {
        let key = paths::client_state_path(client_id);
        let existing = self
            .read_raw(storage, meter, &key)?
            .ok_or_else(|| IbcError::ClientNotFound(client_id.to_string()))?;
        // Preserve chain_id, bump height.
        let s = String::from_utf8_lossy(&existing);
        let chain_id = s
            .split("\"chain_id\":\"")
            .nth(1)
            .and_then(|r| r.split('"').next())
            .unwrap_or("")
            .to_string();
        let state = format!(
            "{{\"chain_id\":\"{}\",\"latest_height\":{}}}",
            chain_id, height
        );
        self.write_raw(storage, meter, &key, state.as_bytes())
    }

    // ---- ICS-3 connection handshake ----

    fn conn_open_init(
        &self,
        storage: &mut dyn Storage,
        meter: &GasMeter,
        client_id: &str,
        connection_id: &str,
        counterparty_client_id: &str,
        counterparty_connection_id: &str,
        counterparty_prefix: &str,
    ) -> PulsarResult<()> {
        let key = paths::connection_path(connection_id);
        if self.read_raw(storage, meter, &key)?.is_some() {
            return Err(IbcError::ConnectionExists(connection_id.to_string()).into());
        }
        let end = ConnectionEnd {
            client_id: client_id.to_string(),
            versions: vec![Version {
                identifier: "1".to_string(),
                features: vec![
                    "ORDER_ORDERED".to_string(),
                    "ORDER_UNORDERED".to_string(),
                ],
            }],
            state: ConnectionState::Init as i32,
            counterparty: Some(ConnectionCounterparty {
                client_id: counterparty_client_id.to_string(),
                connection_id: counterparty_connection_id.to_string(),
                prefix: Some(MerklePrefix {
                    key_prefix: counterparty_prefix.as_bytes().to_vec(),
                }),
            }),
            delay_period: 0,
        };
        self.write_raw(storage, meter, &key, &end.encode_to_vec())
    }

    fn conn_open_ack(
        &self,
        storage: &mut dyn Storage,
        meter: &GasMeter,
        connection_id: &str,
        counterparty_connection_id: &str,
    ) -> PulsarResult<()> {
        let key = paths::connection_path(connection_id);
        let raw = self
            .read_raw(storage, meter, &key)?
            .ok_or_else(|| IbcError::ConnectionNotFound(connection_id.to_string()))?;
        let mut end = ConnectionEnd::decode(raw.as_slice())
            .map_err(|e| IbcError::Invalid(format!("decode ConnectionEnd: {e}")))?;
        if end.state != ConnectionState::Init as i32 {
            return Err(IbcError::InvalidState {
                object: format!("connection {connection_id}"),
                expected: "INIT".to_string(),
                found: format!("{}", end.state),
            }
            .into());
        }
        if let Some(cp) = end.counterparty.as_mut() {
            cp.connection_id = counterparty_connection_id.to_string();
        }
        end.state = ConnectionState::Open as i32;
        self.write_raw(storage, meter, &key, &end.encode_to_vec())
    }

    // ---- ICS-4 channel handshake ----

    #[allow(clippy::too_many_arguments)]
    fn chan_open_init(
        &self,
        storage: &mut dyn Storage,
        meter: &GasMeter,
        port_id: &str,
        channel_id: &str,
        connection_id: &str,
        counterparty_port_id: &str,
        counterparty_channel_id: &str,
        ordering: &str,
        version: &str,
    ) -> PulsarResult<()> {
        // The connection must exist and be OPEN.
        let conn_raw = self
            .read_raw(storage, meter, &paths::connection_path(connection_id))?
            .ok_or_else(|| IbcError::ConnectionNotFound(connection_id.to_string()))?;
        let conn = ConnectionEnd::decode(conn_raw.as_slice())
            .map_err(|e| IbcError::Invalid(format!("decode ConnectionEnd: {e}")))?;
        if conn.state != ConnectionState::Open as i32 {
            return Err(IbcError::ConnectionNotOpen(connection_id.to_string()).into());
        }

        let key = paths::channel_path(port_id, channel_id);
        if self.read_raw(storage, meter, &key)?.is_some() {
            return Err(
                IbcError::ChannelExists(port_id.to_string(), channel_id.to_string()).into(),
            );
        }
        let order = if ordering.eq_ignore_ascii_case("ordered") {
            Order::Ordered
        } else {
            Order::Unordered
        };
        let chan = Channel {
            state: ChannelState::Init as i32,
            ordering: order as i32,
            counterparty: Some(ChannelCounterparty {
                port_id: counterparty_port_id.to_string(),
                channel_id: counterparty_channel_id.to_string(),
            }),
            connection_hops: vec![connection_id.to_string()],
            version: version.to_string(),
        };
        self.write_raw(storage, meter, &key, &chan.encode_to_vec())?;
        // Initialize the send sequence to 1.
        self.write_raw(
            storage,
            meter,
            &paths::next_sequence_send_path(port_id, channel_id),
            &1u64.to_be_bytes(),
        )
    }

    fn chan_open_ack(
        &self,
        storage: &mut dyn Storage,
        meter: &GasMeter,
        port_id: &str,
        channel_id: &str,
        counterparty_channel_id: &str,
        counterparty_version: &str,
    ) -> PulsarResult<()> {
        let key = paths::channel_path(port_id, channel_id);
        let raw = self
            .read_raw(storage, meter, &key)?
            .ok_or_else(|| IbcError::ChannelNotFound(port_id.to_string(), channel_id.to_string()))?;
        let mut chan = Channel::decode(raw.as_slice())
            .map_err(|e| IbcError::Invalid(format!("decode Channel: {e}")))?;
        if chan.state != ChannelState::Init as i32 {
            return Err(IbcError::InvalidState {
                object: format!("channel {port_id}/{channel_id}"),
                expected: "INIT".to_string(),
                found: format!("{}", chan.state),
            }
            .into());
        }
        if let Some(cp) = chan.counterparty.as_mut() {
            cp.channel_id = counterparty_channel_id.to_string();
        }
        chan.version = counterparty_version.to_string();
        chan.state = ChannelState::Open as i32;
        self.write_raw(storage, meter, &key, &chan.encode_to_vec())
    }

    // ---- ICS-20 transfer ----

    #[allow(clippy::too_many_arguments)]
    fn transfer(
        &self,
        storage: &mut dyn Storage,
        meter: &GasMeter,
        block: &BlockInfo,
        sm: &StateMachine,
        sender: &AccountId,
        port_id: &str,
        channel_id: &str,
        token: &Coin,
        receiver: &str,
        timeout_height: u64,
        timeout_timestamp: u64,
    ) -> PulsarResult<(u64, Vec<u8>)> {
        // Channel must be OPEN.
        let chan_raw = self
            .read_raw(storage, meter, &paths::channel_path(port_id, channel_id))?
            .ok_or_else(|| IbcError::ChannelNotFound(port_id.to_string(), channel_id.to_string()))?;
        let chan = Channel::decode(chan_raw.as_slice())
            .map_err(|e| IbcError::Invalid(format!("decode Channel: {e}")))?;
        if chan.state != ChannelState::Open as i32 {
            return Err(
                IbcError::ChannelNotOpen(port_id.to_string(), channel_id.to_string()).into(),
            );
        }

        // Escrow the tokens into the (port, channel) escrow account.
        let escrow = escrow_account(port_id, channel_id);
        sm.bank.transfer(
            storage,
            meter,
            block,
            sm,
            sender.clone(),
            escrow,
            vec![token.clone()],
        )?;

        // Canonical ICS-20 packet data (sorted JSON, matching ibc-go's
        // FungibleTokenPacketData.GetBytes()).
        let packet_data = format!(
            "{{\"amount\":\"{}\",\"denom\":\"{}\",\"receiver\":\"{}\",\"sender\":\"{}\"}}",
            token.amount, token.denom, receiver, sender
        )
        .into_bytes();

        // Read + bump the send sequence.
        let seq_key = paths::next_sequence_send_path(port_id, channel_id);
        let seq_raw = self
            .read_raw(storage, meter, &seq_key)?
            .ok_or_else(|| IbcError::ChannelNotFound(port_id.to_string(), channel_id.to_string()))?;
        let mut seq_bytes = [0u8; 8];
        seq_bytes.copy_from_slice(&seq_raw[..8]);
        let sequence = u64::from_be_bytes(seq_bytes);

        // ibc-go CommitPacket: sha256(u64be(timeout_ts) || u64be(rev_num) ||
        // u64be(rev_height) || sha256(data)). The inner data hash is REQUIRED —
        // ibc-go hashes the payload into a fixed-length preimage, so appending
        // raw data produces a commitment that never matches. We use revision 0
        // and the provided height.
        let data_hash = Sha256::digest(&packet_data);
        let mut buf = Vec::with_capacity(24 + data_hash.len());
        buf.extend_from_slice(&timeout_timestamp.to_be_bytes());
        buf.extend_from_slice(&0u64.to_be_bytes()); // revision_number
        buf.extend_from_slice(&timeout_height.to_be_bytes()); // revision_height
        buf.extend_from_slice(&data_hash);
        let commitment = Sha256::digest(&buf);

        self.write_raw(
            storage,
            meter,
            &paths::packet_commitment_path(port_id, channel_id, sequence),
            &commitment,
        )?;
        self.write_raw(storage, meter, &seq_key, &(sequence + 1).to_be_bytes())?;

        // Store the full packet for the relay daemon — the commitment above is
        // a sha256 hash (not reversible), so the relayer reads the packet itself
        // here via the `proof` query's `value` to rebuild `MsgRecvPacket`.
        let packet = Packet {
            sequence,
            source_port: port_id.to_string(),
            source_channel: channel_id.to_string(),
            destination_port: chan
                .counterparty
                .as_ref()
                .map(|c| c.port_id.clone())
                .unwrap_or_default(),
            destination_channel: chan
                .counterparty
                .as_ref()
                .map(|c| c.channel_id.clone())
                .unwrap_or_default(),
            data: packet_data.clone(),
            timeout_height: Some(Height {
                revision_number: 0,
                revision_height: timeout_height,
            }),
            timeout_timestamp,
        };
        self.write_raw(
            storage,
            meter,
            &paths::packet_data_path(port_id, channel_id, sequence),
            &packet.encode_to_vec(),
        )?;

        Ok((sequence, packet_data))
    }

    // ---- ICS-25 acknowledgement ----

    fn acknowledgement(
        &self,
        storage: &mut dyn Storage,
        meter: &GasMeter,
        port_id: &str,
        channel_id: &str,
        sequence: u64,
    ) -> PulsarResult<()> {
        // Clear the packet commitment (devnet: ack proof carried, not verified).
        storage.remove(meter, &paths::packet_commitment_path(port_id, channel_id, sequence))?;
        storage.remove(meter, &paths::packet_data_path(port_id, channel_id, sequence))?;
        Ok(())
    }

    // ---- ICS-4 timeout ----

    /// Process a packet timeout: refund the escrowed token to the original
    /// sender and clear the commitment + stored packet. The refund target and
    /// amount come from the stored packet data — never from the message — so a
    /// relayer cannot redirect the refund.
    /// (devnet: timeout proof carried, not verified.)
    fn timeout(
        &self,
        storage: &mut dyn Storage,
        meter: &GasMeter,
        block: &BlockInfo,
        sm: &StateMachine,
        port_id: &str,
        channel_id: &str,
        sequence: u64,
    ) -> PulsarResult<(AccountId, Coin)> {
        // Load the stored packet — the commitment path only holds a hash, so we
        // read the full proto `Packet` written at send time.
        let raw = self
            .read_raw(storage, meter, &paths::packet_data_path(port_id, channel_id, sequence))?
            .ok_or_else(|| {
                IbcError::Invalid(format!(
                    "no stored packet to time out at {port_id}/{channel_id}/seq{sequence}"
                ))
            })?;
        let packet = Packet::decode(raw.as_slice())
            .map_err(|e| IbcError::Invalid(format!("decode Packet: {e}")))?;

        // Decode the canonical ICS-20 packet data (sorted JSON) for sender +
        // denom + amount.
        #[derive(serde::Deserialize)]
        struct FungibleTokenPacketData {
            amount: String,
            denom: String,
            sender: String,
        }
        let data: FungibleTokenPacketData = cosmwasm_std::from_json(&packet.data)
            .map_err(|e| IbcError::Invalid(format!("decode packet data: {e}")))?;
        let refund_to = AccountId::parse_string(&data.sender).map_err(|e| {
            IbcError::Invalid(format!("packet sender '{}': {e}", data.sender))
        })?;
        let amount: u128 = data
            .amount
            .parse()
            .map_err(|_| IbcError::Invalid(format!("packet amount '{}'", data.amount)))?;
        let coin = Coin {
            denom: data.denom,
            amount: amount.into(),
        };

        // Refund the escrowed tokens to the original sender.
        let escrow = escrow_account(port_id, channel_id);
        sm.bank.transfer(
            storage,
            meter,
            block,
            sm,
            escrow,
            refund_to.clone(),
            vec![coin.clone()],
        )?;

        // Clear the packet commitment + stored packet (devnet: timeout proof
        // carried, not verified).
        storage.remove(meter, &paths::packet_commitment_path(port_id, channel_id, sequence))?;
        storage.remove(meter, &paths::packet_data_path(port_id, channel_id, sequence))?;

        Ok((refund_to, coin))
    }

    // ---- queries used by the relayer ----

    pub fn get_connection(
        &self,
        storage: &dyn ReadonlyStorage,
        meter: &GasMeter,
        connection_id: &str,
    ) -> PulsarResult<Option<ConnectionEnd>> {
        match self.read_raw(storage, meter, &paths::connection_path(connection_id))? {
            Some(raw) => Ok(Some(
                ConnectionEnd::decode(raw.as_slice())
                    .map_err(|e| IbcError::Invalid(format!("decode ConnectionEnd: {e}")))?,
            )),
            None => Ok(None),
        }
    }

    pub fn get_channel(
        &self,
        storage: &dyn ReadonlyStorage,
        meter: &GasMeter,
        port_id: &str,
        channel_id: &str,
    ) -> PulsarResult<Option<Channel>> {
        match self.read_raw(storage, meter, &paths::channel_path(port_id, channel_id))? {
            Some(raw) => Ok(Some(
                Channel::decode(raw.as_slice())
                    .map_err(|e| IbcError::Invalid(format!("decode Channel: {e}")))?,
            )),
            None => Ok(None),
        }
    }

    // Silence unused-import warning for Height until the counterparty client lands.
    #[allow(dead_code)]
    fn _height(h: u64) -> Height {
        Height {
            revision_number: 0,
            revision_height: h,
        }
    }
}
