use cosmrs::tx::SignerPublicKey;

use crate::TxError;

pub enum PubKey {
    Ed25519(Vec<u8>),
    Secp256k1(Vec<u8>),
}

impl PubKey {
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
}
