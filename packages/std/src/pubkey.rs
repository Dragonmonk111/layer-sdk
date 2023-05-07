use cosmrs::tx::SignerPublicKey;
use cosmwasm_crypto::secp256k1_verify;

use crate::TxError;

#[derive(Debug, PartialEq, Clone)]
pub enum PubKey {
    Ed25519(Vec<u8>),
    Secp256k1(Vec<u8>),
}

impl PubKey {
    // TODO: move to cosmos package
    pub fn parse_cosmos(pubkey: &SignerPublicKey) -> Result<Self, TxError> {
        match pubkey {
            SignerPublicKey::Single(pk) => match pk.type_url() {
                cosmrs::crypto::PublicKey::ED25519_TYPE_URL => Ok(PubKey::Ed25519(pk.to_bytes())),
                cosmrs::crypto::PublicKey::SECP256K1_TYPE_URL => {
                    Ok(PubKey::Secp256k1(pk.to_bytes()))
                }
                url => Err(TxError::UnsupportedPubKey(url)),
            },
            _ => Err(TxError::UnsupportedPubKey("multisig")),
        }
    }

    pub fn validate_signature(&self, message_hash: &[u8], signature: &[u8]) -> Result<(), TxError> {
        match self {
            PubKey::Secp256k1(pk) => {
                if !secp256k1_verify(message_hash, signature, pk.as_slice())
                    .map_err(|_| TxError::InvalidSignature)?
                {
                    Err(TxError::InvalidSignature)
                } else {
                    Ok(())
                }
            }
            PubKey::Ed25519(_) => todo!(),
        }
    }
}
