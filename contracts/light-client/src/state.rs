use cosmwasm_schema::cw_serde;
use cw_storage_plus::{Item, Map};

/// Client state for the JunoClaw BLS threshold light client.
///
/// The validator set is static (Phase A DKG) so the group public key never
/// changes — there is no validator-set-update path in v1. See
/// `docs/BLS_LIGHT_CLIENT_SPEC.md` §4.
#[cw_serde]
pub struct ClientState {
    /// Chain identifier, e.g. "junoclaw-1".
    pub chain_id: String,
    /// BLS12-381 G2 group public key from the Phase A ceremony, 96 bytes
    /// compressed, hex-encoded.
    pub group_public_key_hex: String,
    /// Latest verified height.
    pub latest_height: u64,
    /// Frozen on detected misbehaviour (§7). None while active.
    pub frozen_height: Option<u64>,
}

/// Per-height consensus state derived from a verified header.
#[cw_serde]
pub struct ConsensusState {
    /// sha256 payload digest from the finalized proposal, hex-encoded.
    pub payload_digest_hex: String,
    /// Block timestamp carried alongside the header (unsigned — see spec §11.3).
    pub timestamp: u64,
    /// Consensus epoch the block was finalized in.
    pub epoch: u64,
    /// Consensus view the block was finalized in.
    pub view: u64,
    /// Parent view.
    pub parent: u64,
}

pub const CLIENT_STATE: Item<ClientState> = Item::new("client_state");
pub const CONSENSUS_STATES: Map<u64, ConsensusState> = Map::new("consensus_states");
