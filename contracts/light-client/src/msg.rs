use cosmwasm_schema::{cw_serde, QueryResponses};

use crate::state::{ClientState, ConsensusState};

#[cw_serde]
pub struct InstantiateMsg {
    pub chain_id: String,
    /// 96-byte compressed BLS12-381 G2 group public key, hex-encoded.
    pub group_public_key_hex: String,
}

/// A header submitted by a relayer. `proposal_bytes_hex` and
/// `certificate_bytes_hex` are passed through verbatim from
/// `App::get_block_proposal`/`App::get_block_certificate` — see spec §5.
#[cw_serde]
pub struct Header {
    pub height: u64,
    pub proposal_bytes_hex: String,
    pub certificate_bytes_hex: String,
    pub timestamp: u64,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Verify a header's BLS threshold certificate and, if valid, advance
    /// `latest_height` and store the derived `ConsensusState`.
    UpdateClient { header: Header },
    /// Prove two headers equivocate (same height/round, different payload)
    /// and freeze the client.
    SubmitMisbehaviour { header_a: Header, header_b: Header },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ClientState)]
    ClientState {},
    #[returns(ConsensusState)]
    ConsensusState { height: u64 },
    /// Dry-run verification without mutating state — useful for relayers to
    /// pre-check a header before spending gas on `UpdateClient`.
    #[returns(VerifyHeaderResponse)]
    VerifyHeader { header: Header },
}

#[cw_serde]
pub struct VerifyHeaderResponse {
    pub valid: bool,
    pub reason: Option<String>,
}
