use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Binary;

use crate::state::{Config, EvidenceRecord, KeyHistoryEntry, SeatRecord};

#[cw_serde]
pub struct InstantiateMsg {
    /// cw4 group contract — the authority on who may attest.
    /// Queried via `{"member":{"addr":...}}` on every write.
    pub cw4_group_addr: String,
    /// DAO core contract — revokes, rebinds, reconfigures. A passed DAO
    /// proposal executes as this address (ordinary sender check).
    pub dao_core_addr: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Claim a seat (or rotate the keys of a seat already bound to the
    /// sender). Sender must hold cw4 weight > 0.
    ///
    /// Fails if the seat is bound to a *different* member — rebinding
    /// requires `RevokeAttestation` by the bound member or dao_core.
    UpdateAttestation {
        seat_index: u8,
        /// P2P/peer identity key on the node.
        hot_ed25519: Binary,
        /// BLS12-381 share public key (compressed G2, 96 bytes) — the key
        /// equivocation evidence verifies against.
        bls_share_pubkey: Binary,
        /// MAYO2 public key (PQ partials, same role). Bound now so the
        /// evidence schema can extend to PQ equivocation later.
        mayo_pubkey: Option<Binary>,
        /// 32-byte x-only secp256k1 (BIP-340) — the relay identity.
        npub: Binary,
    },
    /// Revoke a seat binding. Sender must be the bound member_addr or
    /// dao_core_addr.
    RevokeAttestation { seat_index: u8 },
    /// Permissionless: revokes the record iff its member_addr no longer
    /// holds cw4 weight. Refuses while the member is still in the group.
    PruneSeat { seat_index: u8 },
    /// dao_core only — supports the bech32/address migration at the
    /// juno→jclaw cutover.
    UpdateConfig {
        cw4_group_addr: Option<String>,
        dao_core_addr: Option<String>,
    },
    /// Permissionless equivocation evidence. Both signatures are checked
    /// against the seat's bound bls_share_pubkey; the pair is stored for a
    /// DAO proposal to reference. Deduped on (seat_index, view).
    ///
    /// `msg_*` are the *exact byte strings that were hashed to G1* — i.e.
    /// commonware's `union_unique(vote_namespace, vote_message)` =
    /// `varint(len(ns)) || ns || msg`, where `ns` is e.g.
    /// `b"slay3r-consensus-v1" || "_NOTARIZE" | "_NULLIFY" | "_FINALIZE"`.
    /// The contract deliberately does not re-derive namespaces so any
    /// present or future subject type verifies the same way.
    ///
    /// `sig_*` are the 48-byte compressed G1 partial signatures
    /// (`PartialSignature.value` — the participant index prefix is NOT
    /// included; attribution comes from seat_index).
    SubmitEvidence {
        seat_index: u8,
        view: u64,
        msg_a: Binary,
        sig_a: Binary,
        msg_b: Binary,
        sig_b: Binary,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(SeatResponse)]
    GetSeat { seat_index: u8 },

    #[returns(SeatResponse)]
    GetMember { addr: String },

    /// Active (non-pruned, member still holds cw4 weight) seat records —
    /// THIS is the relay allowlist source. Joins the cw4 group
    /// server-side; on group query failure the whole query errors so the
    /// relay keeps its last good set.
    #[returns(SeatsResponse)]
    ListActiveSeats {
        start_after: Option<u8>,
        limit: Option<u32>,
    },

    /// Rotated-out keys for a seat, oldest first, with validity windows
    /// (attested_at..superseded_at) so evidence signed under a superseded
    /// key still resolves to a member.
    #[returns(KeyHistoryResponse)]
    GetKeyHistory { seat_index: u8 },

    /// Stored equivocation evidence, optionally filtered by seat.
    #[returns(EvidenceResponse)]
    ListEvidence { seat_index: Option<u8> },

    #[returns(ConfigResponse)]
    GetConfig {},
}

#[cw_serde]
pub struct SeatResponse {
    pub seat: Option<SeatRecord>,
}

#[cw_serde]
pub struct SeatsResponse {
    pub seats: Vec<SeatRecord>,
}

#[cw_serde]
pub struct KeyHistoryResponse {
    pub history: Vec<KeyHistoryEntry>,
}

#[cw_serde]
pub struct EvidenceResponse {
    pub evidence: Vec<EvidenceRecord>,
}

#[cw_serde]
pub struct ConfigResponse {
    pub config: Config,
}

// ─── cw4 group query surface (subset) ──────────────────────────────────────
// The on-chain group (cw4-group / jclaw-credential) answers
// `{"member":{"addr":...}}` with `{"weight":N|null}` and
// `{"list_members":{...}}` with `{"members":[{addr,weight},...]}`.

#[cw_serde]
pub enum Cw4QueryMsg {
    Member { addr: String },
    ListMembers {
        start_after: Option<String>,
        limit: Option<u32>,
    },
}

#[cw_serde]
pub struct Cw4MemberResponse {
    pub weight: Option<u64>,
}

#[cw_serde]
pub struct Cw4Member {
    pub addr: String,
    pub weight: u64,
}

#[cw_serde]
pub struct Cw4MemberListResponse {
    pub members: Vec<Cw4Member>,
}
