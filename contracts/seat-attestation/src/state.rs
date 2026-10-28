use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Binary};
use cw_storage_plus::{Item, Map};

pub const CONFIG: Item<Config> = Item::new("config");

/// Primary seat records, keyed by consensus validator index.
pub const SEATS: Map<u8, SeatRecord> = Map::new("seats");

/// Secondary index: cw4 member → seat_index (one member holds one seat).
pub const MEMBER_SEAT: Map<Addr, u8> = Map::new("member_seat");

/// Global uniqueness indexes: key bytes → owning seat_index. Every key
/// field is unique across active records — a member cannot bind another
/// seat's npub or hot key.
pub const HOT_KEY_OWNER: Map<Vec<u8>, u8> = Map::new("hot_key");
pub const NPUB_OWNER: Map<Vec<u8>, u8> = Map::new("npub");
pub const BLS_KEY_OWNER: Map<Vec<u8>, u8> = Map::new("bls_key");
pub const MAYO_KEY_OWNER: Map<Vec<u8>, u8> = Map::new("mayo_key");

/// Next history sequence number per seat.
pub const KEY_HISTORY_SEQ: Map<u8, u64> = Map::new("kh_seq");

/// Append-only history of rotated-out hot keys: (seat_index, seq) → entry.
/// Rotated keys keep their validity windows so evidence signed under a
/// superseded key still resolves to the bound member.
pub const KEY_HISTORY: Map<(u8, u64), KeyHistoryEntry> = Map::new("key_history");

/// Equivocation evidence, deduped on (seat_index, view).
pub const EVIDENCE: Map<(u8, u64), EvidenceRecord> = Map::new("evidence");

#[cw_serde]
pub struct Config {
    /// cw4 group contract — queried for member weight on writes.
    pub cw4_group_addr: Addr,
    /// DAO core — admin for revokes and config updates.
    pub dao_core_addr: Addr,
}

#[cw_serde]
pub struct SeatRecord {
    /// Consensus validator index (0..n) — not on-chain verifiable;
    /// first-claim by a weight-holding member, DAO arbitrates disputes.
    pub seat_index: u8,
    /// cw4 member (cold, governance identity).
    pub member_addr: Addr,
    /// P2P/peer identity key on the node.
    pub hot_ed25519: Binary,
    /// BLS12-381 share public key — signs consensus vote partials;
    /// the key equivocation verifies against (compressed G2, 96 bytes).
    pub bls_share_pubkey: Binary,
    /// MAYO2 public key (PQ partials, same role).
    pub mayo_pubkey: Option<Binary>,
    /// 32-byte x-only secp256k1 (BIP-340) — the relay identity.
    pub npub: Binary,
    /// Block time (seconds) when this attestation was written.
    pub attested_at: u64,
}

#[cw_serde]
pub struct KeyHistoryEntry {
    pub seq: u64,
    pub hot_ed25519: Binary,
    pub npub: Binary,
    /// Block time (seconds) the rotated record was originally attested.
    pub attested_at: u64,
    /// Block time (seconds) the keys were superseded.
    pub superseded_at: u64,
}

#[cw_serde]
pub struct EvidenceRecord {
    pub seat_index: u8,
    /// Bound member at submission time — the governance attribution.
    pub member_addr: Addr,
    /// Consensus view at which the conflicting partials were produced.
    pub view: u64,
    /// The exact preimages hashed to G1 (see ExecuteMsg::SubmitEvidence).
    pub msg_a: Binary,
    pub sig_a: Binary,
    pub msg_b: Binary,
    pub sig_b: Binary,
    /// True iff BOTH partials verified against the seat's bound
    /// bls_share_pubkey at submission time. Unverifiable-but-wellformed
    /// submissions still land (verified=false) for governance review.
    pub verified: bool,
    pub submitted_by: Addr,
    /// Block time (seconds) of submission.
    pub submitted_at: u64,
}
