//! NIP-01 event model: identity, id derivation, BIP-340 verification.

use k256::schnorr::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum EventError {
    #[error("invalid hex in field {0}")]
    BadHex(&'static str),
    #[error("id mismatch: declared {declared}, computed {computed}")]
    IdMismatch { declared: String, computed: String },
    #[error("invalid pubkey")]
    BadPubkey,
    #[error("invalid signature")]
    BadSignature,
    #[error("signature verification failed")]
    SigVerifyFailed,
}

/// A Nostr event. `id` commits to (pubkey, created_at, kind, tags, content)
/// via SHA256 of the NIP-01 serialization; `sig` is a BIP-340 Schnorr
/// signature over `id` by `pubkey`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Event {
    pub id: String,
    pub pubkey: String,
    pub created_at: u64,
    pub kind: u64,
    #[serde(default)]
    pub tags: Vec<Vec<String>>,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub sig: String,
}

impl Event {
    /// The NIP-01 serialization committed to by `id`:
    /// `[0, pubkey, created_at, kind, tags, content]` — compact JSON.
    pub fn commitment_preimage(&self) -> Vec<u8> {
        serde_json::to_string(&serde_json::json!([
            0,
            self.pubkey,
            self.created_at,
            self.kind,
            self.tags,
            self.content,
        ]))
        .expect("json serialization of fixed-shape array cannot fail")
        .into_bytes()
    }

    /// The id this event *should* declare.
    pub fn compute_id(&self) -> [u8; 32] {
        Sha256::digest(&self.commitment_preimage()).into()
    }

    /// Declared `id` equals the computed commitment. Cheap; always run
    /// before `verify_sig`.
    pub fn verify_id(&self) -> Result<(), EventError> {
        let declared = hex::decode(&self.id).map_err(|_| EventError::BadHex("id"))?;
        if declared.len() != 32 {
            return Err(EventError::BadHex("id"));
        }
        let computed = self.compute_id();
        if declared.as_slice() != computed {
            return Err(EventError::IdMismatch {
                declared: self.id.clone(),
                computed: hex::encode(computed),
            });
        }
        Ok(())
    }

    /// BIP-340 Schnorr signature check over `id` by `pubkey`.
    /// Caller should `verify_id` first — a valid sig over a mismatched id
    /// still means a bad event.
    pub fn verify_sig(&self) -> Result<(), EventError> {
        let pk_bytes = hex::decode(&self.pubkey).map_err(|_| EventError::BadHex("pubkey"))?;
        let vk = VerifyingKey::from_bytes(
            pk_bytes
                .as_slice()
                .try_into()
                .map_err(|_| EventError::BadPubkey)?,
        )
        .map_err(|_| EventError::BadPubkey)?;
        let sig_bytes = hex::decode(&self.sig).map_err(|_| EventError::BadHex("sig"))?;
        // k256's Signature::try_from PANICS on a wrong-length slice —
        // reject anything != 64 bytes first so a malformed event can't
        // take down the caller's task.
        if sig_bytes.len() != 64 {
            return Err(EventError::BadSignature);
        }
        let sig = Signature::try_from(sig_bytes.as_slice())
            .map_err(|_| EventError::BadSignature)?;
        let id = hex::decode(&self.id).map_err(|_| EventError::BadHex("id"))?;
        if id.len() != 32 {
            return Err(EventError::BadHex("id"));
        }
        vk.verify_raw(&id, &sig)
            .map_err(|_| EventError::SigVerifyFailed)
    }

    /// Full acceptance check: declared id is the commitment AND the sig
    /// covers it.
    pub fn verify(&self) -> Result<(), EventError> {
        self.verify_id()?;
        self.verify_sig()
    }

    /// First value of a single-letter tag (e.g. `h` for the channel id).
    pub fn tag_value(&self, name: &str) -> Option<&str> {
        self.tags.iter().find_map(|t| {
            (t.first().map(String::as_str) == Some(name))
                .then(|| t.get(1).map(String::as_str))
                .flatten()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::schnorr::SigningKey;

    fn signed_event() -> Event {
        // deterministic keypair — do NOT reuse outside tests
        let sk = SigningKey::from_bytes(&[7u8; 32]).unwrap();
        let vk = sk.verifying_key();
        let pubkey = hex::encode(vk.to_bytes());
        let mut e = Event {
            id: String::new(),
            pubkey,
            created_at: 1_700_000_000,
            kind: 9,
            tags: vec![vec!["h".into(), "chan-uuid".into()]],
            content: "hello relay".into(),
            sig: String::new(),
        };
        let id = e.compute_id();
        e.id = hex::encode(id);
        let sig = sk.sign_raw(&id, &[0u8; 32]).unwrap();
        e.sig = hex::encode(sig.to_bytes());
        e
    }

    #[test]
    fn roundtrip_verify() {
        signed_event().verify().unwrap();
    }

    #[test]
    fn tampered_content_breaks_id() {
        let mut e = signed_event();
        e.content = "forged".into();
        assert!(matches!(e.verify_id(), Err(EventError::IdMismatch { .. })));
    }

    #[test]
    fn tampered_id_fails_sig() {
        let mut e = signed_event();
        e.id = hex::encode([0xabu8; 32]);
        assert!(e.verify().is_err());
    }

    #[test]
    fn wrong_key_fails() {
        let mut e = signed_event();
        let other = SigningKey::from_bytes(&[9u8; 32]).unwrap();
        e.pubkey = hex::encode(other.verifying_key().to_bytes());
        assert!(e.verify().is_err());
    }

    #[test]
    fn tag_lookup() {
        let e = signed_event();
        assert_eq!(e.tag_value("h"), Some("chan-uuid"));
        assert_eq!(e.tag_value("e"), None);
    }
}
