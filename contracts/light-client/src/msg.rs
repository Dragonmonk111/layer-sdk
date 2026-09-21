//! 08-wasm contract API wire types.
//!
//! Field names and shapes are taken verbatim from ibc-go's
//! `modules/light-clients/08-wasm/types/contract_api.go` (v11/main, which is
//! API-identical to v8.3+/v9/v10 for these payloads). The Go side marshals
//! payloads with `encoding/json`, where proto-generated structs emit
//! snake_case json tags and `[]byte` fields emit base64 strings — this maps
//! exactly onto `#[cw_serde]` + `cosmwasm_std::Binary`.

use cosmwasm_schema::cw_serde;
use cosmwasm_std::Binary;

use crate::state::Height;

/// Sent to the contract's `instantiate` entry point by the 08-wasm module
/// (`MsgCreateClient` → `NewClientState`).
///
/// `client_state` and `consensus_state` are JSON-encoded
/// [`crate::state::ClientState`] / [`crate::state::ConsensusState`]
/// respectively.
#[cw_serde]
pub struct InstantiateMsg {
    pub client_state: Binary,
    pub consensus_state: Binary,
    pub checksum: Binary,
}

/// The header a relayer submits inside `client_message` (for
/// `update_state`, `verify_client_message`) — JSON-encoded.
///
/// `proposal_bytes` and `certificate_bytes` are passed through verbatim
/// from `App::get_block_proposal(height)` / `App::get_block_certificate(height)`
/// (spec §5).
#[cw_serde]
pub struct Header {
    pub height: Height,
    /// Block timestamp in nanoseconds (IBC convention).
    pub timestamp: u64,
    /// commonware-codec encoded `Proposal { round, parent, payload }`.
    pub proposal_bytes: Binary,
    /// Raw 48-byte compressed G1 threshold signature.
    pub certificate_bytes: Binary,
}

/// The misbehaviour report a relayer submits inside `client_message` (for
/// `update_state_on_misbehaviour`, `check_for_misbehaviour`) —
/// JSON-encoded. Two headers that both verify but equivocate.
#[cw_serde]
pub struct Misbehaviour {
    pub header_a: Header,
    pub header_b: Header,
}

/// `v2.MerklePath` — `key_path` elements are base64-encoded bytes.
#[cw_serde]
pub struct MerklePath {
    pub key_path: Vec<Binary>,
}

/// The contents of `VerifyMembership.proof` — our own format (the field is
/// opaque bytes to ibc-go). JSON, consistent with the rest of the ABI.
///
/// Verification chain: `sha256(payload_bytes)` must equal the consensus
/// state's `payload_digest` (binding the proof to the signed certificate
/// chain) → bincode-parse the payload → `state_root` → walk `siblings`
/// from the leaf `sha256(0x00 || key || value)` to that root.
///
/// The leaf key is the concatenation of `merkle_path.key_path` elements —
/// the chain stores IBC commitments under keys equal to their ICS-24 path.
#[cw_serde]
pub struct MembershipProof {
    /// Full bincode-serialized `BlockPayload` of the block at `height`.
    pub payload_bytes: Binary,
    /// Index of the proven leaf in the sorted leaf list.
    pub leaf_index: u64,
    /// Sibling hashes bottom-up; `null` marks a promotion level (odd node
    /// count — the node moves up unchanged, no hash is applied).
    pub siblings: Vec<Option<Binary>>,
}

/// Sent to the contract's `query` entry point. Externally tagged enum —
/// matches Go's `omitempty`-single-field struct marshaling.
#[cw_serde]
pub enum QueryMsg {
    Status {},
    TimestampAtHeight { height: Height },
    VerifyClientMessage { client_message: Binary },
    CheckForMisbehaviour { client_message: Binary },
}

/// Sent to the contract's `sudo` entry point.
#[cw_serde]
pub enum SudoMsg {
    UpdateState {
        client_message: Binary,
    },
    UpdateStateOnMisbehaviour {
        client_message: Binary,
    },
    VerifyUpgradeAndUpdateState {
        upgrade_client_state: Binary,
        upgrade_consensus_state: Binary,
        proof_upgrade_client: Binary,
        proof_upgrade_consensus_state: Binary,
    },
    VerifyMembership {
        height: Height,
        delay_time_period: u64,
        delay_block_period: u64,
        proof: Binary,
        merkle_path: MerklePath,
        value: Binary,
    },
    VerifyNonMembership {
        height: Height,
        delay_time_period: u64,
        delay_block_period: u64,
        proof: Binary,
        merkle_path: MerklePath,
    },
    MigrateClientStore {},
}

/// `UpdateStateResult` — returned by the `update_state` sudo call.
#[cw_serde]
pub struct UpdateStateResponse {
    pub heights: Vec<Height>,
}

/// `StatusResult` — returned by the `status` query.
#[cw_serde]
pub struct StatusResponse {
    /// One of: "Active", "Frozen", "Expired", "Unknown" (ICS-2 §11).
    pub status: String,
}

/// `TimestampAtHeightResult` — returned by the `timestamp_at_height` query.
#[cw_serde]
pub struct TimestampAtHeightResponse {
    /// Nanoseconds since epoch.
    pub timestamp: u64,
}

/// `CheckForMisbehaviourResult` — returned by the
/// `check_for_misbehaviour` query.
#[cw_serde]
pub struct CheckForMisbehaviourResponse {
    pub found_misbehaviour: bool,
}
