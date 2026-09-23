use cosmwasm_schema::cw_serde;

/// IBC height — matches `ibc-go`'s `clienttypes.Height` wire format
/// (proto-generated Go json tags are snake_case, marshaled with
/// `encoding/json` by the 08-wasm module).
///
/// JunoClaw v1 maps block height `h` to `revision_number: 0,
/// revision_height: h`.
#[cw_serde]
#[derive(Copy)]
pub struct Height {
    pub revision_number: u64,
    pub revision_height: u64,
}

impl Height {
    pub const fn new(revision_number: u64, revision_height: u64) -> Self {
        Self {
            revision_number,
            revision_height,
        }
    }

    pub const fn from_block_height(height: u64) -> Self {
        Self {
            revision_number: 0,
            revision_height: height,
        }
    }
}

/// Client state for the JunoClaw BLS threshold light client.
///
/// The validator set is static (Phase A DKG) so the group public key never
/// changes — there is no validator-set-update path in v1. See
/// `docs/BLS_LIGHT_CLIENT_SPEC.md` §4.
///
/// Wire format: this struct is JSON-encoded into the `client_state` bytes
/// of the 08-wasm `InstantiateMessage` and into the `Data` field of the
/// Go-side `WasmClientState` proto (whose `LatestHeight` must be kept in
/// sync by the relayer when constructing `MsgCreateClient`).
#[cw_serde]
pub struct ClientState {
    /// Chain identifier, e.g. "junoclaw-1".
    pub chain_id: String,
    /// BLS12-381 G2 group public key from the Phase A ceremony, 96 bytes
    /// compressed, hex-encoded.
    pub group_public_key_hex: String,
    /// Latest verified height.
    pub latest_height: Height,
    /// Frozen on detected misbehaviour (§7). None while active.
    pub frozen_height: Option<Height>,
}

/// Per-height consensus state derived from a verified header.
#[cw_serde]
pub struct ConsensusState {
    /// sha256 payload digest from the finalized proposal, hex-encoded.
    pub payload_digest_hex: String,
    /// Block timestamp in **nanoseconds** (IBC convention) — carried
    /// alongside the header by the relayer (unsigned — see spec §11.3).
    pub timestamp: u64,
    /// Consensus epoch the block was finalized in.
    pub epoch: u64,
    /// Consensus view the block was finalized in.
    pub view: u64,
    /// Parent view.
    pub parent: u64,
}
