//! Key loading and BIP-340 signing — the checkpoint signature (over the
//! canonical anchor-package serialization) and the receipt event
//! signature (standard Nostr `sig` over the event id).

use claw_core::Event;
use k256::schnorr::SigningKey;
use sha2::{Digest, Sha256};
use std::path::Path;

use crate::Error;

/// BIP-340 auxiliary randomness — zero, matching every other signer in
/// this codebase (claw-core tests, claw-relay tests). Nonces stay
/// deterministic-but-secret under BIP-340; aux mainly hardens against
/// fault-injection on embedded targets, which this daemon is not.
const AUX: [u8; 32] = [0u8; 32];

/// Load a 32-byte hex secret (`nsec`-style hex, one line) from `path`.
pub fn load_signing_key(path: &Path) -> Result<SigningKey, Error> {
    let raw = std::fs::read_to_string(path)?;
    let s = raw.trim();
    let bytes = hex::decode(s)
        .map_err(|e| Error::Key(format!("{}: bad hex: {e}", path.display())))?;
    let arr: [u8; 32] = bytes.as_slice().try_into().map_err(|_| {
        Error::Key(format!(
            "{}: expected 32-byte secret, got {} bytes",
            path.display(),
            bytes.len()
        ))
    })?;
    SigningKey::from_bytes(&arr).map_err(|e| Error::Key(format!("{}: {e}", path.display())))
}

/// A checkpoint signature over the canonical anchor-package serialization:
/// `BIP-340(SHA256(canonical_json))` by `pubkey`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CheckpointSig {
    pub pubkey_hex: String,
    pub msg_sha256_hex: String,
    pub sig_hex: String,
}

pub fn sign_checkpoint(sk: &SigningKey, canonical_json: &str) -> Result<CheckpointSig, Error> {
    let digest: [u8; 32] = Sha256::digest(canonical_json.as_bytes()).into();
    let sig = sk
        .sign_raw(&digest, &AUX)
        .map_err(|e| Error::Key(format!("checkpoint sign failed: {e}")))?;
    Ok(CheckpointSig {
        pubkey_hex: hex::encode(sk.verifying_key().to_bytes()),
        msg_sha256_hex: hex::encode(digest),
        sig_hex: hex::encode(sig.to_bytes()),
    })
}

/// Build a fully-signed kind-9 channel event (id + sig filled in) — the
/// receipt posted back to the channel after committed-state verification.
pub fn make_event(sk: &SigningKey, channel_uuid: &str, content: String, now: u64) -> Event {
    let mut e = Event {
        id: String::new(),
        pubkey: hex::encode(sk.verifying_key().to_bytes()),
        created_at: now,
        kind: 9,
        tags: vec![vec!["h".into(), channel_uuid.into()]],
        content,
        sig: String::new(),
    };
    let id = e.compute_id();
    e.id = hex::encode(id);
    e.sig = hex::encode(
        sk.sign_raw(&id, &AUX)
            .expect("sign_raw cannot fail with a valid key")
            .to_bytes(),
    );
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::schnorr::VerifyingKey;
    use sha2::Digest;

    fn test_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32]).unwrap()
    }

    #[test]
    fn checkpoint_sig_verifies() {
        let sk = test_key();
        let s = sign_checkpoint(&sk, r#"{"format":"x","count":3}"#).unwrap();
        let vk = VerifyingKey::from_bytes(&sk.verifying_key().to_bytes()).unwrap();
        let digest: [u8; 32] = Sha256::digest(br#"{"format":"x","count":3}"#).into();
        let sig = k256::schnorr::Signature::try_from(hex::decode(&s.sig_hex).unwrap().as_slice())
            .unwrap();
        vk.verify_raw(&digest, &sig).unwrap();
        assert_eq!(s.pubkey_hex, hex::encode(sk.verifying_key().to_bytes()));
        assert_eq!(s.msg_sha256_hex, hex::encode(digest));
    }

    #[test]
    fn receipt_event_verifies() {
        let e = make_event(&test_key(), "chan-uuid", "receipt".into(), 1_700_000_000);
        e.verify().unwrap();
        assert_eq!(e.tag_value("h"), Some("chan-uuid"));
        assert_eq!(e.kind, 9);
    }

    #[test]
    fn load_key_from_file() {
        let dir = std::env::temp_dir().join(format!("claw-cp-key-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("k.hex");
        std::fs::write(&p, format!("{}\n", hex::encode([9u8; 32]))).unwrap();
        let sk = load_signing_key(&p).unwrap();
        assert_eq!(
            hex::encode(sk.verifying_key().to_bytes()),
            hex::encode(SigningKey::from_bytes(&[9u8; 32]).unwrap().verifying_key().to_bytes())
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_key_rejects_bad_file() {
        let dir = std::env::temp_dir().join(format!("claw-cp-badkey-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("k.hex");
        std::fs::write(&p, "zzzz").unwrap();
        assert!(load_signing_key(&p).is_err());
        std::fs::write(&p, hex::encode([1u8; 16])).unwrap();
        assert!(load_signing_key(&p).is_err()); // wrong length
        std::fs::remove_dir_all(&dir).ok();
    }
}
