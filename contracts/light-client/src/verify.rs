//! BLS12-381 threshold certificate verification core.
//!
//! Implements spec §6 (`docs/BLS_LIGHT_CLIENT_SPEC.md`) using the pure-Rust
//! `bls12_381` crate (no host precompile — see spec §6 for rationale).
//!
//! MinSig variant: signatures live in G1 (48 bytes compressed), the group
//! public key lives in G2 (96 bytes compressed). Verification is a single
//! pairing check:
//!
//!   e(certificate, G2::generator) == e(H(namespace || proposal_bytes), pubkey)
//!
//! matching `commonware_cryptography::bls12381::primitives::ops::verify_message::<MinSig>`
//! exactly (cross-checked against `tools/verify-cert`).

use bls12_381::hash_to_curve::{ExpandMsgXmd, HashToCurve};
use bls12_381::{pairing, G1Affine, G1Projective, G2Affine};

use crate::error::ContractError;

/// Base consensus namespace, from `slay3rd`'s `Namespace` config.
const BASE_NAMESPACE: &[u8] = b"slay3r-consensus-v1";
/// Suffix applied to derive the finalize-specific namespace.
const FINALIZE_SUFFIX: &[u8] = b"_FINALIZE";
/// Domain separation tag for hashing a message to G1 under the MinSig
/// variant (see `commonware_cryptography::bls12381::primitives::group::G1_MESSAGE`).
const G1_MESSAGE_DST: &[u8] = b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_POP_";

/// Fields decoded out of the raw `Proposal` bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedProposal {
    pub epoch: u64,
    pub view: u64,
    pub parent: u64,
    pub payload: [u8; 32],
}

/// The finalize namespace: `concat(base_namespace, "_FINALIZE")` (plain
/// concatenation, no length prefix — this differs from `union_unique` below,
/// which IS length-prefixed and is applied on top of this namespace).
fn finalize_namespace() -> Vec<u8> {
    let mut ns = BASE_NAMESPACE.to_vec();
    ns.extend_from_slice(FINALIZE_SUFFIX);
    ns
}

/// `commonware_utils::union_unique(namespace, msg)`:
/// `uvarint(namespace.len()) || namespace || msg`.
fn union_unique(namespace: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(10 + namespace.len() + msg.len());
    write_uvarint(&mut buf, namespace.len() as u64);
    buf.extend_from_slice(namespace);
    buf.extend_from_slice(msg);
    buf
}

/// Unsigned LEB128 varint encoder, matching `commonware_codec::varint`.
fn write_uvarint(buf: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if value == 0 {
            break;
        }
    }
}

/// Unsigned LEB128 varint decoder, matching `commonware_codec::varint`.
fn read_uvarint(buf: &[u8], pos: &mut usize) -> Result<u64, ContractError> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    loop {
        let byte = *buf.get(*pos).ok_or(ContractError::InvalidProposal)?;
        *pos += 1;
        if shift >= 64 {
            return Err(ContractError::InvalidProposal);
        }
        result |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    Ok(result)
}

/// Decode `Proposal { round: Round { epoch, view }, parent, payload }`.
///
/// `epoch`, `view`, and `parent` are `commonware_codec::varint::UInt`
/// encoded; `payload` is a fixed 32-byte sha256 digest written last.
pub fn decode_proposal(bytes: &[u8]) -> Result<DecodedProposal, ContractError> {
    let mut pos = 0usize;
    let epoch = read_uvarint(bytes, &mut pos)?;
    let view = read_uvarint(bytes, &mut pos)?;
    let parent = read_uvarint(bytes, &mut pos)?;
    let payload_slice = bytes
        .get(pos..pos + 32)
        .ok_or(ContractError::InvalidProposal)?;
    if pos + 32 != bytes.len() {
        // Trailing bytes after the payload digest indicate a malformed or
        // unexpected encoding — reject rather than silently truncate.
        return Err(ContractError::InvalidProposal);
    }
    let mut payload = [0u8; 32];
    payload.copy_from_slice(payload_slice);
    Ok(DecodedProposal {
        epoch,
        view,
        parent,
        payload,
    })
}

/// Verify a BLS12-381 threshold certificate over the given proposal bytes
/// against the group public key. Returns the decoded proposal fields on
/// success.
pub fn verify_certificate(
    group_public_key: &[u8],
    proposal_bytes: &[u8],
    certificate_bytes: &[u8],
) -> Result<DecodedProposal, ContractError> {
    if group_public_key.len() != 96 {
        return Err(ContractError::InvalidPublicKeyLength(group_public_key.len()));
    }
    if certificate_bytes.len() != 48 {
        return Err(ContractError::InvalidCertificateLength(certificate_bytes.len()));
    }

    let decoded = decode_proposal(proposal_bytes)?;

    let mut pk_bytes = [0u8; 96];
    pk_bytes.copy_from_slice(group_public_key);
    let pubkey: G2Affine =
        Option::from(G2Affine::from_compressed(&pk_bytes)).ok_or(ContractError::InvalidPublicKey)?;

    let mut sig_bytes = [0u8; 48];
    sig_bytes.copy_from_slice(certificate_bytes);
    let signature: G1Affine =
        Option::from(G1Affine::from_compressed(&sig_bytes)).ok_or(ContractError::InvalidCertificate)?;

    let namespace = finalize_namespace();
    let msg = union_unique(&namespace, proposal_bytes);

    let hm: G1Projective =
        <G1Projective as HashToCurve<ExpandMsgXmd<sha2_09::Sha256>>>::hash_to_curve(&msg, G1_MESSAGE_DST);
    let hm_affine = G1Affine::from(hm);

    let lhs = pairing(&signature, &G2Affine::generator());
    let rhs = pairing(&hm_affine, &pubkey);

    if lhs == rhs {
        Ok(decoded)
    } else {
        Err(ContractError::VerificationFailed)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    use commonware_codec::Encode;
    use commonware_cryptography::bls12381::{
        dkg, primitives::ops::threshold, primitives::variant::MinSig,
    };
    use commonware_parallel::Sequential;
    use commonware_utils::{test_rng_seeded, NZU32, N3f1};

    /// Encodes `Proposal { round: { epoch, view }, parent, payload }` using
    /// the exact same varint-then-fixed-digest layout as
    /// `commonware_consensus::simplex::types::Proposal::write` (verified by
    /// reading `Round`/`Epoch`/`View`'s `Write` impls directly — see spec
    /// §2). Avoids pulling in `commonware-consensus` as a dev-dependency
    /// (transitively drags in aws-lc-sys, which needs NASM to build).
    fn encode_proposal(epoch: u64, view: u64, parent: u64, payload: [u8; 32]) -> Vec<u8> {
        let mut buf = Vec::new();
        write_uvarint(&mut buf, epoch);
        write_uvarint(&mut buf, view);
        write_uvarint(&mut buf, parent);
        buf.extend_from_slice(&payload);
        buf
    }

    /// Deals a real threshold key + shares, signs a real `Proposal`, and
    /// recovers a real threshold signature — the exact same code path
    /// `slay3rd` uses — then verifies it with our pure-Rust contract-side
    /// verifier. This is the cross-implementation check that de-risks §6.
    pub(crate) fn build_certified_proposal(
        seed: u64,
        height_epoch: u64,
        height_view: u64,
        parent_view: u64,
        payload: [u8; 32],
    ) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let mut rng = test_rng_seeded(seed);
        let n = 5u32;
        let (sharing, shares) =
            dkg::deal_anonymous::<MinSig, N3f1>(&mut rng, Default::default(), NZU32!(n));

        let proposal_bytes = encode_proposal(height_epoch, height_view, parent_view, payload);

        let namespace = finalize_namespace();
        let partials: Vec<_> = shares
            .iter()
            .map(|s| threshold::sign_message::<MinSig>(s, &namespace, &proposal_bytes))
            .collect();
        let certificate =
            threshold::recover::<MinSig, _, N3f1>(&sharing, &partials, &Sequential).unwrap();

        let group_public_key = sharing.public().encode().to_vec();
        let certificate_bytes = certificate.encode().to_vec();

        (group_public_key, proposal_bytes, certificate_bytes)
    }

    #[test]
    fn verify_certificate_accepts_real_threshold_signature() {
        let payload = [7u8; 32];
        let (pubkey, proposal_bytes, cert_bytes) = build_certified_proposal(0, 0, 42, 41, payload);

        let decoded = verify_certificate(&pubkey, &proposal_bytes, &cert_bytes)
            .expect("real threshold signature must verify");

        assert_eq!(decoded.epoch, 0);
        assert_eq!(decoded.view, 42);
        assert_eq!(decoded.parent, 41);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn verify_certificate_rejects_wrong_payload() {
        let (pubkey, proposal_bytes, cert_bytes) = build_certified_proposal(0, 0, 1, 0, [1u8; 32]);

        // Tamper with the payload digest (last 32 bytes) after signing.
        let mut tampered = proposal_bytes.clone();
        let len = tampered.len();
        tampered[len - 32..].copy_from_slice(&[2u8; 32]);

        let err = verify_certificate(&pubkey, &tampered, &cert_bytes).unwrap_err();
        assert!(matches!(err, ContractError::VerificationFailed));
    }

    #[test]
    fn verify_certificate_rejects_wrong_pubkey() {
        let (_pubkey, proposal_bytes, cert_bytes) = build_certified_proposal(0, 0, 1, 0, [3u8; 32]);
        let (other_pubkey, _, _) = build_certified_proposal(1, 0, 1, 0, [3u8; 32]);

        let err = verify_certificate(&other_pubkey, &proposal_bytes, &cert_bytes).unwrap_err();
        assert!(matches!(err, ContractError::VerificationFailed));
    }

    #[test]
    fn decode_proposal_roundtrips_fields() {
        let (_pubkey, proposal_bytes, _cert) = build_certified_proposal(0, 3, 1000, 999, [9u8; 32]);
        let decoded = decode_proposal(&proposal_bytes).unwrap();
        assert_eq!(decoded.epoch, 3);
        assert_eq!(decoded.view, 1000);
        assert_eq!(decoded.parent, 999);
        assert_eq!(decoded.payload, [9u8; 32]);
    }

    #[test]
    fn decode_proposal_rejects_truncated_bytes() {
        let (_pubkey, proposal_bytes, _cert) = build_certified_proposal(0, 0, 1, 0, [0u8; 32]);
        let truncated = &proposal_bytes[..proposal_bytes.len() - 1];
        assert!(matches!(
            decode_proposal(truncated),
            Err(ContractError::InvalidProposal)
        ));
    }
}
