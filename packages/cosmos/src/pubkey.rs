use cosmrs::tx::SignerPublicKey;
use pulsar_std::{PubKey, TxError};

pub fn parse_cosmos_pubkey(pubkey: &SignerPublicKey) -> Result<PubKey, TxError> {
    match pubkey {
        SignerPublicKey::Single(pk) => match pk.type_url() {
            cosmrs::crypto::PublicKey::ED25519_TYPE_URL => Ok(PubKey::Ed25519(pk.to_bytes())),
            cosmrs::crypto::PublicKey::SECP256K1_TYPE_URL => Ok(PubKey::Secp256k1(pk.to_bytes())),
            url => Err(TxError::UnsupportedPubKey(url)),
        },
        _ => Err(TxError::UnsupportedPubKey("multisig")),
    }
}
