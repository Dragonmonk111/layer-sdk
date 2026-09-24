//! ICS-24 commitment path builders.
//!
//! JunoClaw stores IBC commitments under keys equal to the path the
//! counterparty's light client reconstructs by concatenating the `MerklePath`
//! `key_path` elements (no separator — see light-client `verify_membership`).
//! ibc-go prepends the counterparty `MerklePrefix` as `key_path[0]`, so the
//! relayer sets that prefix to [`STORE_PREFIX`] (`"ibc/"`) and every commitment
//! lives under `ibc/<ics24_path>`. The concatenation then reproduces the exact
//! storage key.

/// Counterparty commitment prefix — the `MerklePrefix.key_prefix` the relayer
/// sets on the connection. ibc-go prepends it to each ICS-24 path, so the
/// stored key is `ibc/<path>`.
pub const STORE_PREFIX: &str = "ibc/";

/// `ibc/clients/{client_id}/clientState`
pub fn client_state_path(client_id: &str) -> Vec<u8> {
    format!("{}clients/{}/clientState", STORE_PREFIX, client_id).into_bytes()
}

/// `ibc/clients/{client_id}/consensusStates/{revision}-{height}`
pub fn consensus_state_path(client_id: &str, revision: u64, height: u64) -> Vec<u8> {
    format!(
        "{}clients/{}/consensusStates/{}-{}",
        STORE_PREFIX, client_id, revision, height
    )
    .into_bytes()
}

/// `ibc/connections/{connection_id}`
pub fn connection_path(connection_id: &str) -> Vec<u8> {
    format!("{}connections/{}", STORE_PREFIX, connection_id).into_bytes()
}

/// `ibc/channelEnds/ports/{port_id}/channels/{channel_id}`
pub fn channel_path(port_id: &str, channel_id: &str) -> Vec<u8> {
    format!(
        "{}channelEnds/ports/{}/channels/{}",
        STORE_PREFIX, port_id, channel_id
    )
    .into_bytes()
}

/// `ibc/commitments/ports/{port_id}/channels/{channel_id}/sequences/{sequence}`
pub fn packet_commitment_path(port_id: &str, channel_id: &str, sequence: u64) -> Vec<u8> {
    format!(
        "{}commitments/ports/{}/channels/{}/sequences/{}",
        STORE_PREFIX, port_id, channel_id, sequence
    )
    .into_bytes()
}

/// `ibc/acks/ports/{port_id}/channels/{channel_id}/sequences/{sequence}`
pub fn packet_ack_path(port_id: &str, channel_id: &str, sequence: u64) -> Vec<u8> {
    format!(
        "{}acks/ports/{}/channels/{}/sequences/{}",
        STORE_PREFIX, port_id, channel_id, sequence
    )
    .into_bytes()
}

/// `ibc/receipts/ports/{port_id}/channels/{channel_id}/sequences/{sequence}`
pub fn packet_receipt_path(port_id: &str, channel_id: &str, sequence: u64) -> Vec<u8> {
    format!(
        "{}receipts/ports/{}/channels/{}/sequences/{}",
        STORE_PREFIX, port_id, channel_id, sequence
    )
    .into_bytes()
}

/// `ibc/nextSequenceSend/ports/{port_id}/channels/{channel_id}`
pub fn next_sequence_send_path(port_id: &str, channel_id: &str) -> Vec<u8> {
    format!(
        "{}nextSequenceSend/ports/{}/channels/{}",
        STORE_PREFIX, port_id, channel_id
    )
    .into_bytes()
}
