use crate::TxError;
use cosmrs::tx::SignerPublicKey;

// TODO: clarify this logic and put it somewhere
pub enum PubKey {
    Ed25519([u8; 32]),
    Secp256k1(Vec<u8>),
}

impl PubKey {
    pub fn parse_cosmos(_pubkey: &SignerPublicKey) -> Result<Self, TxError> {
        todo!()
    }
}
