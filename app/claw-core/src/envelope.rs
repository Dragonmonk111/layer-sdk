//! Versioned signature envelope for relay frames — the crypto-agility slot.
//!
//! Same structure as the chain's certificate envelope: a single leading tag
//! byte selects the signature suite, and suites whose parameters evolve
//! (MAYO) carry a second variant byte so parameter sets migrate underneath
//! without breaking historical verification.
//!
//! ```text
//! FrameSig   := tag || body
//!   tag 0x01 := BIP-340 Schnorr, body = sig[64]
//!   tag 0x02 := MAYO (post-quantum), body = variant[1] || u32be len || bytes
//!   tag 0x00 := reserved/invalid — decoding it is always an error
//!
//! SignedFrame := pubkey[32] || u32be payload_len || payload || FrameSig
//! ```
//!
//! **Parse now, verify when the suite lands.** The default build verifies
//! Schnorr signatures and *parses* MAYO envelopes (so frames already on the
//! wire round-trip losslessly), but `verify` on a MAYO signature returns
//! [`EnvelopeError::UnsupportedSuite`] — honest stub, no pretend-verify.
//! When the MAYO verifier lands (a `#[cfg]`-gated `sriracha-mayo` dep, or a
//! verifier trait impl), only this module's `verify` arm changes — the wire
//! format and every stored frame stay valid.
//!
//! `SignedFrame` is deliberately NOT a Nostr [`Event`](crate::Event): it's
//! the envelope for checkpoint frames, peer-auth frames, and future control
//! messages that sign a caller-supplied 32-byte digest rather than a
//! NIP-01 commitment.

use k256::schnorr::{Signature, VerifyingKey};

/// Suite tag: BIP-340 Schnorr over secp256k1 (the only suite that verifies
/// in this build).
pub const SUITE_SCHNORR: u8 = 0x01;
/// Suite tag: MAYO post-quantum signature. Parseable today; the verifier
/// plugs in later. `variant` selects the parameter set (mirrors the cert
/// envelope's `mayo_variant` registry).
pub const SUITE_MAYO: u8 = 0x02;

#[derive(Debug, thiserror::Error)]
pub enum EnvelopeError {
    /// Leading tag byte names no known suite (incl. reserved 0x00).
    #[error("unknown signature suite tag 0x{0:02x}")]
    UnknownSuite(u8),
    /// Suite is recognized and parsed, but this build has no verifier for
    /// it. `(suite_tag, variant)`.
    #[error("signature suite 0x{0:02x} variant {1} is parseable but not verifiable in this build")]
    UnsupportedSuite(u8, u8),
    /// Truncated, over-long, or otherwise non-canonical encoding.
    #[error("malformed envelope encoding")]
    Malformed,
    /// Suite verified but the signature (or pubkey) is bad.
    #[error("signature verification failed")]
    BadSignature,
}

/// A detached signature with its suite tag. Decoding is canonical: the
/// input must be consumed exactly, no trailing bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameSig {
    /// Tag `0x01` — BIP-340 Schnorr signature bytes.
    Schnorr([u8; 64]),
    /// Tag `0x02` — MAYO. `variant` selects the parameter set so future
    /// MAYO parameter migrations don't change the tag.
    Mayo { variant: u8, bytes: Vec<u8> },
}

impl FrameSig {
    /// The suite tag byte this signature encodes under.
    pub fn suite_tag(&self) -> u8 {
        match self {
            FrameSig::Schnorr(_) => SUITE_SCHNORR,
            FrameSig::Mayo { .. } => SUITE_MAYO,
        }
    }

    /// `tag || body` — see module docs for the exact layout.
    pub fn encode(&self) -> Vec<u8> {
        match self {
            FrameSig::Schnorr(sig) => {
                let mut out = Vec::with_capacity(1 + 64);
                out.push(SUITE_SCHNORR);
                out.extend_from_slice(sig);
                out
            }
            FrameSig::Mayo { variant, bytes } => {
                let len = u32::try_from(bytes.len()).expect("sig > u32::MAX bytes");
                let mut out = Vec::with_capacity(1 + 1 + 4 + bytes.len());
                out.push(SUITE_MAYO);
                out.push(*variant);
                out.extend_from_slice(&len.to_be_bytes());
                out.extend_from_slice(bytes);
                out
            }
        }
    }

    /// Canonical decode: `bytes` must be exactly one encoded signature —
    /// unknown tags error as [`EnvelopeError::UnknownSuite`], truncation or
    /// trailing bytes as [`EnvelopeError::Malformed`].
    pub fn decode(bytes: &[u8]) -> Result<FrameSig, EnvelopeError> {
        let (&tag, body) = bytes.split_first().ok_or(EnvelopeError::Malformed)?;
        match tag {
            SUITE_SCHNORR => {
                let sig: &[u8; 64] = body.try_into().map_err(|_| EnvelopeError::Malformed)?;
                Ok(FrameSig::Schnorr(*sig))
            }
            SUITE_MAYO => {
                if body.len() < 1 + 4 {
                    return Err(EnvelopeError::Malformed);
                }
                let variant = body[0];
                let len = u32::from_be_bytes(body[1..5].try_into().unwrap()) as usize;
                let sig = &body[5..];
                if sig.len() != len {
                    return Err(EnvelopeError::Malformed);
                }
                Ok(FrameSig::Mayo {
                    variant,
                    bytes: sig.to_vec(),
                })
            }
            other => Err(EnvelopeError::UnknownSuite(other)),
        }
    }
}

/// A generic relay frame: 32-byte author pubkey, opaque payload, tagged
/// signature. `verify` checks the sig over a caller-supplied digest — the
/// digest commitment (what the payload is hashed with, domain separators,
/// frame context) is the caller's business, same as `Event::verify_sig`
/// trusting `verify_id` ran first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedFrame {
    pub pubkey: [u8; 32],
    pub payload: Vec<u8>,
    pub sig: FrameSig,
}

impl SignedFrame {
    /// `pubkey[32] || u32be payload_len || payload || FrameSig::encode()`.
    pub fn encode(&self) -> Vec<u8> {
        let len = u32::try_from(self.payload.len()).expect("payload > u32::MAX bytes");
        let sig = self.sig.encode();
        let mut out = Vec::with_capacity(32 + 4 + self.payload.len() + sig.len());
        out.extend_from_slice(&self.pubkey);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&self.payload);
        out.extend_from_slice(&sig);
        out
    }

    /// Inverse of [`encode`](Self::encode); rejects truncation, length
    /// mismatches, and any `FrameSig` decode error.
    pub fn decode(bytes: &[u8]) -> Result<SignedFrame, EnvelopeError> {
        if bytes.len() < 32 + 4 {
            return Err(EnvelopeError::Malformed);
        }
        let pubkey: [u8; 32] = bytes[..32].try_into().unwrap();
        let payload_len =
            u32::from_be_bytes(bytes[32..36].try_into().unwrap()) as usize;
        let rest = &bytes[36..];
        if rest.len() < payload_len {
            return Err(EnvelopeError::Malformed);
        }
        let (payload, sig_bytes) = rest.split_at(payload_len);
        let sig = FrameSig::decode(sig_bytes)?;
        Ok(SignedFrame {
            pubkey,
            payload: payload.to_vec(),
            sig,
        })
    }

    /// Verify the frame signature over `digest`.
    ///
    /// - `Schnorr` → BIP-340 verify of `sig` over `digest` under `pubkey`
    ///   (same `verify_raw` path as `Event::verify_sig`).
    /// - `Mayo` → [`EnvelopeError::UnsupportedSuite`]. The envelope parses
    ///   today; an honest error stands in for the verifier until the MAYO
    ///   suite lands. Callers must not treat this as "verified".
    pub fn verify(&self, digest: &[u8; 32]) -> Result<(), EnvelopeError> {
        match &self.sig {
            FrameSig::Schnorr(sig_bytes) => {
                let vk = VerifyingKey::from_bytes(&self.pubkey)
                    .map_err(|_| EnvelopeError::BadSignature)?;
                let sig = Signature::try_from(sig_bytes.as_slice())
                    .map_err(|_| EnvelopeError::BadSignature)?;
                vk.verify_raw(digest, &sig)
                    .map_err(|_| EnvelopeError::BadSignature)
            }
            FrameSig::Mayo { variant, .. } => {
                Err(EnvelopeError::UnsupportedSuite(SUITE_MAYO, *variant))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::schnorr::SigningKey;

    // deterministic keypair — do NOT reuse outside tests
    fn test_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32]).unwrap()
    }

    fn schnorr_frame(digest: &[u8; 32]) -> SignedFrame {
        let sk = test_key();
        SignedFrame {
            pubkey: sk.verifying_key().to_bytes().into(),
            payload: b"checkpoint-frame".to_vec(),
            sig: FrameSig::Schnorr(sk.sign_raw(digest, &[0u8; 32]).unwrap().to_bytes()),
        }
    }

    #[test]
    fn schnorr_sig_roundtrip() {
        let sig = FrameSig::Schnorr([0xabu8; 64]);
        assert_eq!(FrameSig::decode(&sig.encode()).unwrap(), sig);
    }

    #[test]
    fn mayo_sig_roundtrip() {
        let sig = FrameSig::Mayo {
            variant: 2,
            bytes: vec![0xde, 0xad, 0xbe, 0xef],
        };
        let enc = sig.encode();
        assert_eq!(&enc[..6], &[0x02, 0x02, 0, 0, 0, 4]);
        assert_eq!(FrameSig::decode(&enc).unwrap(), sig);
    }

    #[test]
    fn mayo_empty_bytes_roundtrip() {
        let sig = FrameSig::Mayo {
            variant: 1,
            bytes: Vec::new(),
        };
        assert_eq!(FrameSig::decode(&sig.encode()).unwrap(), sig);
    }

    #[test]
    fn frame_roundtrip_both_variants() {
        let digest = [0x42u8; 32];
        for frame in [
            schnorr_frame(&digest),
            SignedFrame {
                pubkey: [0x11u8; 32],
                payload: b"peer-auth".to_vec(),
                sig: FrameSig::Mayo {
                    variant: 2,
                    bytes: vec![1, 2, 3],
                },
            },
        ] {
            assert_eq!(SignedFrame::decode(&frame.encode()).unwrap(), frame);
        }
    }

    #[test]
    fn unknown_and_reserved_tags_rejected() {
        for tag in [0x00u8, 0x03, 0x7f, 0xff] {
            let err = FrameSig::decode(&[tag, 0, 0, 0, 0]).unwrap_err();
            assert_eq!(err.to_string(), EnvelopeError::UnknownSuite(tag).to_string());
            assert!(matches!(err, EnvelopeError::UnknownSuite(t) if t == tag));
        }
    }

    #[test]
    fn truncated_mayo_rejected() {
        // header promises 4 bytes, only 2 present
        let mut enc = FrameSig::Mayo {
            variant: 2,
            bytes: vec![0xaa; 4],
        }
        .encode();
        enc.truncate(enc.len() - 2);
        assert!(matches!(
            FrameSig::decode(&enc),
            Err(EnvelopeError::Malformed)
        ));
        // trailing garbage after declared len
        let mut enc = FrameSig::Mayo {
            variant: 2,
            bytes: vec![0xaa; 4],
        }
        .encode();
        enc.push(0xbb);
        assert!(matches!(
            FrameSig::decode(&enc),
            Err(EnvelopeError::Malformed)
        ));
        // variant byte but no len header
        assert!(matches!(
            FrameSig::decode(&[SUITE_MAYO, 0x02, 0x00]),
            Err(EnvelopeError::Malformed)
        ));
        // tag only
        assert!(matches!(
            FrameSig::decode(&[SUITE_MAYO]),
            Err(EnvelopeError::Malformed)
        ));
    }

    #[test]
    fn truncated_schnorr_and_frame_rejected() {
        // 63 sig bytes instead of 64
        let mut enc = FrameSig::Schnorr([0xaau8; 64]).encode();
        enc.truncate(enc.len() - 1);
        assert!(matches!(
            FrameSig::decode(&enc),
            Err(EnvelopeError::Malformed)
        ));
        // empty input
        assert!(matches!(
            FrameSig::decode(&[]),
            Err(EnvelopeError::Malformed)
        ));
        // frame shorter than fixed header
        assert!(matches!(
            SignedFrame::decode(&[0u8; 35]),
            Err(EnvelopeError::Malformed)
        ));
        // payload_len overruns buffer
        let mut enc = schnorr_frame(&[0x42u8; 32]).encode();
        enc[35] = 0xff; // payload_len 16 -> 255 overruns the buffer
        assert!(matches!(
            SignedFrame::decode(&enc),
            Err(EnvelopeError::Malformed)
        ));
    }

    #[test]
    fn schnorr_verify_happy_path() {
        let digest = [0x42u8; 32];
        schnorr_frame(&digest).verify(&digest).unwrap();
    }

    #[test]
    fn schnorr_forged_rejected() {
        let digest = [0x42u8; 32];
        // sig over a different digest
        let frame = schnorr_frame(&digest);
        assert!(matches!(
            frame.verify(&[0x43u8; 32]),
            Err(EnvelopeError::BadSignature)
        ));
        // wrong pubkey
        let mut frame = schnorr_frame(&digest);
        frame.pubkey = SigningKey::from_bytes(&[9u8; 32])
            .unwrap()
            .verifying_key()
            .to_bytes()
            .into();
        assert!(matches!(
            frame.verify(&digest),
            Err(EnvelopeError::BadSignature)
        ));
        // garbage sig bytes
        let mut frame = schnorr_frame(&digest);
        frame.sig = FrameSig::Schnorr([0x00u8; 64]);
        assert!(matches!(
            frame.verify(&digest),
            Err(EnvelopeError::BadSignature)
        ));
    }

    #[test]
    fn mayo_verify_is_honest_stub() {
        let frame = SignedFrame {
            pubkey: [0x11u8; 32],
            payload: b"future-pq".to_vec(),
            sig: FrameSig::Mayo {
                variant: 2,
                bytes: vec![0xaa; 1312], // MAYO-2 sig-sized, still not verifiable
            },
        };
        match frame.verify(&[0x42u8; 32]) {
            Err(EnvelopeError::UnsupportedSuite(tag, variant)) => {
                assert_eq!(tag, SUITE_MAYO);
                assert_eq!(variant, 2);
            }
            other => panic!("expected UnsupportedSuite, got {other:?}"),
        }
    }
}
