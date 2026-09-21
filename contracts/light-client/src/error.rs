use cosmwasm_std::StdError;

#[derive(thiserror::Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("client is frozen at height {0} (misbehaviour detected)")]
    ClientFrozen(u64),

    #[error("header height {header} is not greater than latest verified height {latest}")]
    NonMonotonicHeight { header: u64, latest: u64 },

    #[error("group public key must be exactly 96 bytes (compressed G2 point), got {0}")]
    InvalidPublicKeyLength(usize),

    #[error("group public key is not a valid compressed G2 point")]
    InvalidPublicKey,

    #[error("certificate must be exactly 48 bytes (compressed G1 point), got {0}")]
    InvalidCertificateLength(usize),

    #[error("certificate is not a valid compressed G1 point")]
    InvalidCertificate,

    #[error("proposal bytes are malformed (truncated varint or wrong length)")]
    InvalidProposal,

    #[error("BLS12-381 pairing check failed — certificate does not verify against the group public key")]
    VerificationFailed,

    #[error("no consensus state stored for height {0}")]
    ConsensusStateNotFound(u64),

    #[error("misbehaviour headers must be for the same height or round")]
    MisbehaviourMismatch,

    #[error("no equivocation found between the two supplied headers")]
    NoEquivocation,

    #[error("membership/non-membership proofs are not yet supported: the batch commitment layout (spec §8, §11.1) is not yet pinned down")]
    MembershipProofsUnsupported,

    #[error("operation is not supported by this client (v1)")]
    Unsupported,
}
