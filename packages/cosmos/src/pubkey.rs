use cosmrs::tendermint::PublicKey as TendermintPublicKey;
use cosmrs::tx::SignerPublicKey;
use cosmrs::Any;

use layer_std::{PubKey, QueryError, TxError, HYBRID_PUBKEY_TYPE_URL};

pub fn parse_cosmos_pubkey(pubkey: &SignerPublicKey) -> Result<PubKey, TxError> {
    match pubkey {
        SignerPublicKey::Single(pk) => match pk.type_url() {
            cosmrs::crypto::PublicKey::ED25519_TYPE_URL => Ok(PubKey::ed25519(pk.to_bytes())),
            cosmrs::crypto::PublicKey::SECP256K1_TYPE_URL => Ok(PubKey::secp256k1(pk.to_bytes())),
            url => Err(TxError::UnsupportedPubKey(url)),
        },
        // Custom key types (e.g. the hybrid secp256k1+MAYO key) arrive as a
        // generic Any — cosmrs doesn't know the type_url, we do.
        SignerPublicKey::Any(any) if any.type_url == HYBRID_PUBKEY_TYPE_URL => {
            PubKey::from_hybrid_any_bytes(&any.value)
                .ok_or(TxError::UnsupportedPubKey("malformed hybrid pubkey"))
        }
        _ => Err(TxError::UnsupportedPubKey("multisig")),
    }
}

pub fn encode_cosmos_pubkey(pubkey: &PubKey) -> Result<Any, QueryError> {
    if let Some(value) = pubkey.to_hybrid_any_bytes() {
        return Ok(Any {
            type_url: HYBRID_PUBKEY_TYPE_URL.to_string(),
            value,
        });
    }
    let pk = match pubkey {
        PubKey::Ed25519(pk) => TendermintPublicKey::from_raw_ed25519(pk),
        PubKey::Secp256k1(pk) => TendermintPublicKey::from_raw_secp256k1(pk),
        PubKey::HybridSecp256k1Mayo { .. } => unreachable!("hybrid handled above"),
    }
    .ok_or_else(|| QueryError::EncodingError("invalid pubkey".to_string()))?;

    // FIXME: map error to some generic line not the undeterministic report line
    cosmrs::crypto::PublicKey::from(pk)
        .to_any()
        .map_err(|e| QueryError::EncodingError(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mayo_test_vector::{MSG_HEX, PK_HEX, SIG_HEX};
    use k256::ecdsa::{signature::hazmat::PrehashSigner, Signature, SigningKey};
    use layer_std::MayoVariant;
    use rand_chacha::ChaCha20Rng;
    use rand_core::SeedableRng;

    /// End-to-end hybrid auth: a MAYO-2 sig (reference-impl vector) + a
    /// secp256k1 sig over the same 32-byte message hash; the account-side
    /// `validate_signature` must accept only when BOTH halves verify.
    #[test]
    fn hybrid_pubkey_verifies_both_signatures() {
        let mut rng = ChaCha20Rng::from_seed([7; 32]);
        let message_hash = hex::decode(MSG_HEX).unwrap();
        let mayo_pk = hex::decode(PK_HEX).unwrap();
        let mayo_sig = hex::decode(SIG_HEX).unwrap();

        // secp256k1 half — signed over the same message_hash
        let secp_sk = SigningKey::random(&mut rng);
        let secp_pk = secp_sk
            .verifying_key()
            .to_encoded_point(true)
            .as_bytes()
            .to_vec();
        let secp_sig: Signature = secp_sk.sign_prehash(&message_hash).unwrap();

        let pk = PubKey::hybrid(secp_pk, MayoVariant::Mayo2, mayo_pk);
        let sig = PubKey::pack_hybrid_signature(&secp_sig.to_bytes(), &mayo_sig);

        pk.validate_signature(&message_hash, &sig)
            .expect("valid hybrid signature rejected");

        // Corrupt the MAYO half — must reject
        let mut bad = sig.to_vec();
        *bad.last_mut().unwrap() ^= 0xff;
        assert!(pk.validate_signature(&message_hash, &bad).is_err());

        // Corrupt the secp256k1 half — must reject
        let mut bad = sig.to_vec();
        bad[10] ^= 0xff;
        assert!(pk.validate_signature(&message_hash, &bad).is_err());

        // MAYO sig alone (missing secp prefix) — must reject
        assert!(pk.validate_signature(&message_hash, &mayo_sig).is_err());

        // Wrong message — must reject
        assert!(pk.validate_signature(&[0u8; 32], &sig).is_err());
    }

    /// The hybrid pubkey must survive the Cosmos `Any` round-trip through
    /// `SignerPublicKey::Any` with our custom type_url.
    #[test]
    fn hybrid_pubkey_any_roundtrip() {
        let pk = PubKey::hybrid(vec![2u8; 33], MayoVariant::Mayo5, vec![9u8; 5554]);
        let any = encode_cosmos_pubkey(&pk).unwrap();
        assert_eq!(any.type_url, HYBRID_PUBKEY_TYPE_URL);

        let parsed = parse_cosmos_pubkey(&SignerPublicKey::Any(any)).unwrap();
        assert_eq!(parsed, pk);

        // Hybrid account_id is domain-separated from the secp256k1 space
        let secp_pk = PubKey::secp256k1(vec![2u8; 33]);
        assert_ne!(
            pk.account_id().unwrap(),
            secp_pk.account_id().unwrap(),
            "hybrid and secp256k1 keys must not share an address"
        );
    }
}
