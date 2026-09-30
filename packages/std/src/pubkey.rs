use ripemd::Ripemd160;
use sha2::{Digest, Sha256};
use tracing::debug_span;

use cosmwasm_crypto::secp256k1_verify;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Binary;

use crate::{AccountId, AccountIdError, TxError};

/// Type URL for the hybrid secp256k1+MAYO pubkey when embedded in a Cosmos
/// `Any` (SignerInfo.public_key). The `value` is NOT protobuf — it is the
/// canonical wire encoding produced by `PubKey::to_hybrid_any_bytes`:
///   [mayo_variant: u8][secp_len: u8][secp pk][mayo pk...]
pub const HYBRID_PUBKEY_TYPE_URL: &str = "/junoclaw.crypto.HybridSecp256k1Mayo";

/// Domain separator folded into the hybrid account_id preimage so hybrid
/// addresses can never collide with the plain secp256k1 address space.
const HYBRID_ACCOUNT_DOMAIN: &[u8] = b"junoclaw-hybrid-v1";

/// Signature layout for hybrid txs: first 64 bytes are the secp256k1 compact
/// signature, the remainder is the MAYO signature for `mayo_variant`.
const SECP256K1_SIG_BYTES: usize = 64;

/// MAYO variant identifiers used in `mayo_variant`.
#[cw_serde]
#[derive(Copy, Eq)]
#[repr(u8)]
pub enum MayoVariant {
    Mayo1 = 1,
    Mayo2 = 2,
    Mayo3 = 3,
    Mayo5 = 5,
}

impl MayoVariant {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            1 => Some(MayoVariant::Mayo1),
            2 => Some(MayoVariant::Mayo2),
            3 => Some(MayoVariant::Mayo3),
            5 => Some(MayoVariant::Mayo5),
            _ => None,
        }
    }

    /// Verify a MAYO signature of this variant. Returns false on any
    /// malformed input — never panics.
    fn verify(&self, msg: &[u8], sig: &[u8], pk: &[u8]) -> bool {
        use junoclaw_mayo_verify::{verify, Mayo1, Mayo2, Mayo3, Mayo5};
        match self {
            MayoVariant::Mayo1 => verify::<Mayo1>(msg, sig, pk).unwrap_or(false),
            MayoVariant::Mayo2 => verify::<Mayo2>(msg, sig, pk).unwrap_or(false),
            MayoVariant::Mayo3 => verify::<Mayo3>(msg, sig, pk).unwrap_or(false),
            MayoVariant::Mayo5 => verify::<Mayo5>(msg, sig, pk).unwrap_or(false),
        }
    }
}

// TODO: make this binary not Vec<u8>
#[cw_serde]
#[derive(Eq)]
pub enum PubKey {
    Ed25519(Binary),
    Secp256k1(Binary),
    /// Hybrid post-quantum account key: a spend requires BOTH a valid
    /// secp256k1 signature AND a valid MAYO signature over the same
    /// message hash. Security = max(classical, PQ): the account stays safe
    /// even if secp256k1 is broken (or MAYO is found flawed).
    HybridSecp256k1Mayo {
        secp256k1: Binary,
        mayo_variant: MayoVariant,
        mayo_pk: Binary,
    },
}

impl PubKey {
    pub fn ed25519(pk: impl Into<Binary>) -> Self {
        PubKey::Ed25519(pk.into())
    }

    pub fn secp256k1(pk: impl Into<Binary>) -> Self {
        PubKey::Secp256k1(pk.into())
    }

    pub fn hybrid(
        secp256k1: impl Into<Binary>,
        mayo_variant: MayoVariant,
        mayo_pk: impl Into<Binary>,
    ) -> Self {
        PubKey::HybridSecp256k1Mayo {
            secp256k1: secp256k1.into(),
            mayo_variant,
            mayo_pk: mayo_pk.into(),
        }
    }

    /// Canonical wire encoding placed in `Any.value` for the hybrid key type.
    /// Layout: [variant: 1][secp_len: 1][secp pk][mayo pk].
    pub fn to_hybrid_any_bytes(&self) -> Option<Vec<u8>> {
        let PubKey::HybridSecp256k1Mayo {
            secp256k1,
            mayo_variant,
            mayo_pk,
        } = self
        else {
            return None;
        };
        let secp = secp256k1.as_slice();
        let secp_len: u8 = secp.len().try_into().ok()?;
        let mut out = Vec::with_capacity(2 + secp.len() + mayo_pk.len());
        out.push(*mayo_variant as u8);
        out.push(secp_len);
        out.extend_from_slice(secp);
        out.extend_from_slice(mayo_pk);
        Some(out)
    }

    /// Inverse of `to_hybrid_any_bytes`. Returns None on malformed input.
    pub fn from_hybrid_any_bytes(bytes: &[u8]) -> Option<Self> {
        let variant = MayoVariant::from_byte(*bytes.first()?)?;
        let secp_len = *bytes.get(1)? as usize;
        let secp = bytes.get(2..2 + secp_len)?;
        let mayo_pk = bytes.get(2 + secp_len..)?;
        if secp.is_empty() || mayo_pk.is_empty() {
            return None;
        }
        Some(PubKey::HybridSecp256k1Mayo {
            secp256k1: Binary::from(secp),
            mayo_variant: variant,
            mayo_pk: Binary::from(mayo_pk),
        })
    }

    /// Build the hybrid signature blob: 64-byte secp256k1 compact sig then
    /// the raw MAYO signature bytes.
    pub fn pack_hybrid_signature(secp256k1_sig: &[u8], mayo_sig: &[u8]) -> Binary {
        let mut out = Vec::with_capacity(SECP256K1_SIG_BYTES + mayo_sig.len());
        out.extend_from_slice(secp256k1_sig);
        out.extend_from_slice(mayo_sig);
        Binary::from(out)
    }

    pub fn validate_signature(&self, message_hash: &[u8], signature: &[u8]) -> Result<(), TxError> {
        let _span = debug_span!("validate_signature").entered();
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
            PubKey::HybridSecp256k1Mayo {
                secp256k1,
                mayo_variant,
                mayo_pk,
            } => {
                // Both halves must verify; failure of either rejects the tx.
                let Some((secp_sig, mayo_sig)) = signature.split_at_checked(SECP256K1_SIG_BYTES)
                else {
                    return Err(TxError::InvalidSignature);
                };
                let secp_ok = secp256k1_verify(message_hash, secp_sig, secp256k1.as_slice())
                    .unwrap_or(false);
                let mayo_ok = mayo_variant.verify(message_hash, mayo_sig, mayo_pk.as_slice());
                if secp_ok && mayo_ok {
                    Ok(())
                } else {
                    Err(TxError::InvalidSignature)
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
            PubKey::HybridSecp256k1Mayo { .. } => {
                // sha256(domain || canonical encoding) then ripemd160 —
                // domain-separated from plain secp256k1 addresses so the two
                // key types can never claim the same account.
                let wire = self.to_hybrid_any_bytes().unwrap_or_default();
                let mut preimage = Vec::with_capacity(HYBRID_ACCOUNT_DOMAIN.len() + wire.len());
                preimage.extend_from_slice(HYBRID_ACCOUNT_DOMAIN);
                preimage.extend_from_slice(&wire);
                let sha_digest = Sha256::digest(&preimage);
                let ripemd_digest = Ripemd160::digest(&sha_digest[..]);
                AccountId::new(ripemd_digest.as_slice())
            }
            PubKey::Ed25519(_) => todo!(),
        }
    }
}
