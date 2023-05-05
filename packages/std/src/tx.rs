use cosmos_sdk_proto::prost::DecodeError;
use cosmrs::ErrorReport;
use thiserror::Error;

use crate::addr::Addr;
use crate::msg::{Msg, MsgError};

pub use cosmos::{CosmosTx, SigningInfo, FeeInfo};

/// A list of various tx formats we accept.
/// We start with Cosmos-SDK format for compatibility, but want to later allow native signing format.
/// We can pass this around to auth to allow handling multiple types
pub enum Tx {
    /// Cosmos Format. Note that we support a subset of the functionality:
    /// Only one signer, no authz or fee grants. But that means 90%+ of tx work, and are "Keplr compatible"
    /// TODO: timeout height...
    Cosmos(CosmosTx),
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum TxError {
    #[error("{0}")]
    Msg(#[from] MsgError),

    #[error("No signer on the tx")]
    NoSigner,

    #[error("More than one signer on this tx")]
    MultipleSigners,

    #[error("Trying to pay fees in multiple denoms, we allow only 1")]
    MultipleFeeDenoms,

    // TODO: remove this and replace with deterministic errors
    #[error("{0}")]
    ProtoDecode(#[from] DecodeError),

    // TODO: remove this and replace with deterministic errors
    #[error("Error Report: {0}")]
    ErrorReport(String),
}

impl From<ErrorReport> for TxError {
    fn from(value: ErrorReport) -> Self {
        TxError::ErrorReport(value.to_string())
    }
}

/// Information to execute the contents of the transaction after it has passed auth
pub struct ExecInfo {
    pub msgs: Vec<Msg>,
    pub signer: Addr,
}

impl Tx {
    pub fn parse_tx(bytes: &[u8], chain_id: &str) -> Result<Self, TxError> {
        // TODO: add other loop if non-cosmos
        let tx = CosmosTx::parse_cosmos(bytes, chain_id)?;
        Ok(Tx::Cosmos(tx))
    }
}

pub mod cosmos {
    use super::*;
    use cosmos_sdk_proto::cosmos::tx::v1beta1::TxRaw;
    use cosmos_sdk_proto::prost::Message;

    use cosmrs::tx::SignDoc;
    use cosmwasm_std::{coin, Coin};

    use crate::pubkey::PubKey;
    use crate::required_signer;

    pub const FIXED_ACCOUNT_NUMBER: u64 = 0;

    // This is parsed from cosmrs::Raw and cosmrs::Tx
    pub struct CosmosTx {
        // These are the raw bytes that should be properly signed by a pubkey to be valid
        pub sign_bytes: Vec<u8>,

        // The decoded messages inside this transaction
        pub msgs: Vec<Msg>,

        // The account that will execute them (must match the pubkey in signing_info if that is set)
        pub signer: Addr,

        /// this is info on the signer (pubkey, sequence)
        pub signing_info: SigningInfo,

        /// this is info on the fee (amount and gas wanted)
        pub fee: FeeInfo,

        /// if set and the chain height is greater than this, abort the tx in all cases
        pub timeout_height: Option<u64>,
    }

    impl CosmosTx {
        /// Parses the raw cosmos tx encoding and calculate the expected sign bytes.
        /// Extracts all useful info from the Tx in a simpler format for us
        pub fn parse_cosmos(bytes: &[u8], chain_id: &str) -> Result<Self, TxError> {
            // get raw format for accurate signing info (to validate sig)
            let raw = TxRaw::decode(bytes)?;
            // FIXME: add tx hash here as well from TxRaw?
            let doc = SignDoc {
                body_bytes: raw.body_bytes,
                auth_info_bytes: raw.auth_info_bytes,
                chain_id: chain_id.to_string(),
                account_number: FIXED_ACCOUNT_NUMBER,
            };
            let sign_bytes = doc.into_bytes()?;

            // parse into cosmrs::Tx so we can understand what we have
            let tx = cosmrs::Tx::from_bytes(bytes)?;
            let msgs: Result<Vec<_>, _> = tx.body.messages.iter().map(Msg::from_cosmos).collect();
            let msgs = msgs?;

            let signer = required_signer(&msgs)?;

            let timeout_height = match tx.body.timeout_height.value() {
                0 => None,
                v => Some(v),
            };

            // TODO: signing info with signature
            let fee = get_fee(&tx)?;

            let signing_info = get_signing_info(&tx)?;

            Ok(CosmosTx {
                sign_bytes,
                signer,
                msgs,
                signing_info,
                fee,
                timeout_height,
            })
        }
    }

    pub struct SigningInfo {
        pub sequence: u64,
        pub pubkey: Option<PubKey>,
        pub signature: Vec<u8>,
    }

    pub fn get_signing_info(tx: &cosmrs::Tx) -> Result<SigningInfo, TxError> {
        let sigs = &tx.signatures;
        let signature = match sigs.len() {
            0 => Err(TxError::NoSigner),
            1 => Ok(sigs[0].clone()),
            _ => Err(TxError::MultipleSigners),
        }?;

        let infos = &tx.auth_info.signer_infos;
        let info = match infos.len() {
            0 => Err(TxError::NoSigner),
            1 => Ok(&infos[0]),
            _ => Err(TxError::MultipleSigners),
        }?;
        let sequence = info.sequence;

        let pubkey = info
            .public_key
            .as_ref()
            .map(PubKey::parse_cosmos)
            .transpose()?;

        Ok(SigningInfo {
            sequence,
            pubkey,
            signature,
        })
    }

    pub struct FeeInfo {
        // how much they pay
        pub fee: Option<Coin>,
        // how much work to do
        pub gas_limit: u64,
    }

    fn parse_fee_coin(fee_coin: &cosmrs::Coin) -> Coin {
        coin(fee_coin.amount, fee_coin.denom.as_ref())
    }

    pub fn get_fee(tx: &cosmrs::Tx) -> Result<FeeInfo, TxError> {
        let info = &tx.auth_info.fee;
        let fee = match info.amount.len() {
            0 => None,
            1 => Some(parse_fee_coin(&info.amount[0])),
            _ => {
                return Err(TxError::MultipleFeeDenoms);
            }
        };
        let gas_limit = info.gas_limit;
        Ok(FeeInfo { fee, gas_limit })
    }
}
