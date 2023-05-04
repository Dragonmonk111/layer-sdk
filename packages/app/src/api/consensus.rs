// based on https://github.com/informalsystems/tendermint-rs/blob/mikhail/cometbft-0.38/proto/src/prost/v0_38/tendermint.types.rs
// but let's make this more generic

use cosmwasm_std::Timestamp;

// This is tendermint control stuff... we may need to abstract this in the future
// Removed many of those items we likely don't change
pub struct ConsensusParams {
    pub block: BlockParams,
    pub evidence: EvidenceParams,
    pub version: u64,
}

pub struct BlockParams {
    pub max_bytes: u64,
    /// None means no limit
    pub max_gas: Option<u64>,
}

pub struct EvidenceParams {
    pub max_age_blocks: u64,
    pub max_age_time: Timestamp,
    pub max_bytes: u64,
}
