use ripemd::Ripemd160;
use sha2::{Digest, Sha256};

use cosmwasm_crypto::secp256k1_verify;
use cosmwasm_schema::cw_serde;

use crate::{AccountId, AccountIdError, TxError};

// TODO: make this binary not Vec<u8>
#[cw_serde]
#[derive(Eq)]
pub enum PubKey {
    Ed25519(Vec<u8>),
    Secp256k1(Vec<u8>),
}

impl PubKey {
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

    // TODO: add test cases for this from some test vectors
    pub fn account_id(&self) -> Result<AccountId, AccountIdError> {
        match self {
            PubKey::Secp256k1(pk) => {
                let sha_digest = Sha256::digest(pk);
                let ripemd_digest = Ripemd160::digest(&sha_digest[..]);
                AccountId::new(ripemd_digest.as_slice())
            }
            PubKey::Ed25519(_) => todo!(),
        }
    }
}
