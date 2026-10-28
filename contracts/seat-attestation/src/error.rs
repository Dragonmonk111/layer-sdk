use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Payment(#[from] cw_utils::PaymentError),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("sender holds no weight in the cw4 group")]
    NotMember,

    #[error("seat {seat_index} is bound to a different member")]
    SeatBoundToOtherMember { seat_index: u8 },

    #[error("member already holds seat {seat_index}; one member may hold only one seat")]
    MemberAlreadyBound { seat_index: u8 },

    #[error("seat {seat_index} has no attestation")]
    SeatNotFound { seat_index: u8 },

    #[error("member of seat {seat_index} still holds cw4 weight")]
    MemberStillActive { seat_index: u8 },

    #[error("evidence for seat {seat_index} at view {view} was already submitted")]
    DuplicateEvidence { seat_index: u8, view: u64 },

    #[error("msg_a and msg_b are identical; not equivocation")]
    NotEquivocating,

    #[error("key material is already bound to seat {seat_index}")]
    KeyInUse { seat_index: u8 },

    #[error("key material must be non-empty")]
    EmptyKey,
}
