use cosmos_sdk_proto::prost::DecodeError;
use cosmrs::ErrorReport;
use thiserror::Error;

use crate::addr::Addr;
use crate::msg::{Msg, MsgError};

pub use cosmos::{CosmosTx, FeeInfo, SigningInfo};

/// A list of various tx formats we accept.
/// We start with Cosmos-SDK format for compatibility, but want to later allow native signing format.
/// We can pass this around to auth to allow handling multiple types
pub enum Tx {
    /// Cosmos Format. Note that we support a subset of the functionality:
    /// Only one signer, no authz or fee grants. But that means 90%+ of tx work, and are "Keplr compatible"
    /// TODO: timeout height...
    Cosmos(CosmosTx),
}

#[derive(Error, Debug, PartialEq)]
pub enum TxError {
    #[error("{0}")]
    Msg(#[from] MsgError),

    #[error("No signer on the tx")]
    NoSigner,

    #[error("More than one signer on this tx")]
    MultipleSigners,

    #[error("Trying to pay fees in multiple denoms, we allow only 1")]
    MultipleFeeDenoms,

    #[error("No support for complex pubkey types: {0}")]
    UnsupportedPubKey(&'static str),

    #[error("No support for signing mode: {0}")]
    UnsupportedSigningMode(&'static str),

    #[error("Tx extensions present but not supported")]
    ExtensionsNotSupported,

    #[error("Fee grant field used but not supported")]
    FeeGrantNotSupported,

    #[error("Fee payer field used but not supported")]
    FeePayerNotSupported,

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
    use cosmos_sdk_proto::cosmos::tx::signing::v1beta1::SignMode;
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

            // other needed info
            let fee = get_fee(&tx)?;
            let signing_info = get_signing_info(&tx)?;
            let timeout_height = match tx.body.timeout_height.value() {
                0 => None,
                v => Some(v),
            };

            // TODO: validate other fields not used from body
            if !tx.body.extension_options.is_empty() {
                return Err(TxError::ExtensionsNotSupported);
            }

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

        // assert we have sign-mode-direct (need to add legacy amino support later)
        match info.mode_info {
            cosmrs::tx::mode_info::ModeInfo::Single(s) => match s.mode {
                SignMode::Direct => Ok(()),
                SignMode::LegacyAminoJson => Err(TxError::UnsupportedSigningMode("legacy_amino")),
                m => Err(TxError::UnsupportedSigningMode(m.as_str_name())),
            },
            _ => Err(TxError::UnsupportedSigningMode("multi")),
        }?;

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

        // assert some fields empty
        if info.granter.is_some() {
            return Err(TxError::FeeGrantNotSupported);
        }
        if info.payer.is_some() {
            return Err(TxError::FeePayerNotSupported);
        }

        Ok(FeeInfo { fee, gas_limit })
    }

    #[cfg(test)]
    mod test {
        use super::*;

        use crate::DEFAULT_BECH32_PREFIX;
        use cosmrs::{
            bank::MsgSend,
            crypto::secp256k1,
            tx::{self, Fee, Msg, SignDoc, SignerInfo},
            Coin,
        };

        #[test]
        fn happy_path_tx_parsing() {
            let sender_private_key = secp256k1::SigningKey::random();
            let sender_public_key = sender_private_key.public_key();
            let sender_account_id = sender_public_key.account_id(DEFAULT_BECH32_PREFIX).unwrap();

            let rcpt_account_id = secp256k1::SigningKey::random()
                .public_key()
                .account_id(DEFAULT_BECH32_PREFIX)
                .unwrap();

            let sequence_number = 5;
            let chain_id = "tpulsar-1".parse().unwrap();
            let gas = 350_000u64;
            let timeout_height = 9001u16;

            let amount = Coin {
                amount: 1_000_000u128,
                denom: "uatom".parse().unwrap(),
            };
            let fee = Coin {
                amount: 200_000u128,
                denom: "uatom".parse().unwrap(),
            };

            let msg_send = MsgSend {
                from_address: sender_account_id.clone(),
                to_address: rcpt_account_id,
                amount: vec![amount.clone()],
            };

            let tx_body = tx::Body::new(vec![msg_send.to_any().unwrap()], "", timeout_height);
            let signer_info = SignerInfo::single_direct(Some(sender_public_key), sequence_number);
            let auth_info = signer_info.auth_info(Fee::from_amount_and_gas(fee.clone(), gas));

            // The "sign doc" contains a message to be signed.
            let sign_doc =
                SignDoc::new(&tx_body, &auth_info, &chain_id, FIXED_ACCOUNT_NUMBER).unwrap();
            // Sign the "sign doc" with the sender's private key, producing a signed raw transaction.
            let tx_signed = sign_doc.sign(&sender_private_key).unwrap();
            // Serialize the raw transaction as bytes (i.e. `Vec<u8>`).
            let tx_bytes = tx_signed.to_bytes().unwrap();

            // now let's parse and see if we have the proper values
            let tx = crate::Tx::parse_tx(&tx_bytes, chain_id.as_str()).unwrap();

            // validate we have the expected values
            let tx = match tx {
                crate::Tx::Cosmos(cms) => cms,
            };
            assert_eq!(tx.timeout_height, Some(timeout_height as u64));
        }
    }
}
