//! Local verification of finality records served by peers.
//!
//! A finality record is what the lightclient `Block` endpoint serves: the
//! encoded simplex `Proposal`, the encoded finalization certificate, and the
//! `BlockPayload` bytes whose sha256 is the proposal's payload digest. The
//! validator set is static and every node holds it in its key file, so a
//! joiner can check a record without trusting the peer that served it.

use commonware_codec::{extensions::DecodeExt, Decode};
use commonware_consensus::simplex::scheme::Scheme;
use commonware_consensus::simplex::types::{Finalization, Proposal};
use commonware_cryptography::certificate::Scheme as CertificateScheme;
use commonware_cryptography::sha256;
use commonware_parallel::Sequential;
use sha2::{Digest as _, Sha256};

/// Signature of a finality check: `(proposal_bytes, certificate_bytes,
/// payload_bytes)` → `Ok(())` when the record is certified.
pub type FinalityCheck = dyn Fn(&[u8], &[u8], &[u8]) -> Result<(), String> + Send + Sync;

/// Checks that `certificate_bytes` is a valid finalization certificate under
/// `scheme`'s validator set for the proposal in `proposal_bytes`, and that
/// the proposal finalizes exactly `payload_bytes`.
pub fn verify_finality<S: Scheme<sha256::Digest>>(
    scheme: &S,
    proposal_bytes: &[u8],
    certificate_bytes: &[u8],
    payload_bytes: &[u8],
) -> Result<(), String> {
    let proposal = Proposal::<sha256::Digest>::decode(proposal_bytes)
        .map_err(|e| format!("proposal decode failed: {e}"))?;
    let digest: [u8; 32] = Sha256::digest(payload_bytes).into();
    if proposal.payload.0 != digest {
        return Err("payload bytes do not match the finalized proposal digest".to_string());
    }
    let certificate = <S as CertificateScheme>::Certificate::decode_cfg(
        certificate_bytes,
        &scheme.certificate_codec_config(),
    )
    .map_err(|e| format!("certificate decode failed: {e}"))?;
    let finalization = Finalization::<S, sha256::Digest> {
        proposal,
        certificate,
    };
    if !finalization.verify(&mut rand::rngs::OsRng, scheme, &Sequential) {
        return Err(
            "finalization certificate does not verify against the configured validator set"
                .to_string(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use commonware_codec::codec::Encode as _;
    use commonware_consensus::simplex::scheme::bls12381_threshold::standard::Scheme as BlsScheme;
    use commonware_consensus::simplex::types::Subject;
    use commonware_consensus::types::{Epoch, Round, View};
    use commonware_cryptography::bls12381::dkg::deal_anonymous;
    use commonware_cryptography::bls12381::primitives::{sharing::Mode, variant::MinSig};
    use commonware_cryptography::{ed25519, Signer as _};
    use commonware_math::algebra::Random as _;
    use commonware_utils::{ordered::Set, N3f1};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use std::num::NonZeroU32;

    type Bls = BlsScheme<ed25519::PublicKey, MinSig>;
    const NAMESPACE: &[u8] = b"slay3r-consensus-v1";

    fn validator_set(seed: u64) -> Vec<Bls> {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut keys: Vec<ed25519::PublicKey> = (0..4)
            .map(|_| ed25519::PrivateKey::random(&mut rng).public_key())
            .collect();
        keys.sort_by(|a, b| {
            let a: &[u8] = a.as_ref();
            let b: &[u8] = b.as_ref();
            a.cmp(b)
        });
        let participants = Set::try_from(keys.as_slice()).unwrap();
        let (sharing, shares) = deal_anonymous::<MinSig, N3f1>(
            &mut rng,
            Mode::NonZeroCounter,
            NonZeroU32::new(4).unwrap(),
        );
        shares
            .into_iter()
            .map(|share| Bls::signer(NAMESPACE, participants.clone(), sharing.clone(), share).unwrap())
            .collect()
    }

    fn proposal(view: u64, payload: &[u8]) -> Proposal<sha256::Digest> {
        let digest: [u8; 32] = Sha256::digest(payload).into();
        Proposal::new(
            Round::new(Epoch::new(0), View::new(view)),
            View::new(view - 1),
            sha256::Digest::from(digest),
        )
    }

    fn certify(set: &[Bls], proposal: &Proposal<sha256::Digest>) -> Vec<u8> {
        let attestations: Vec<_> = set
            .iter()
            .take(3)
            .map(|s| {
                s.sign::<sha256::Digest>(Subject::Finalize { proposal })
                    .unwrap()
            })
            .collect();
        set[0]
            .assemble::<_, N3f1>(attestations, &Sequential)
            .unwrap()
            .encode()
            .to_vec()
    }

    #[test]
    fn certified_record_verifies() {
        let set = validator_set(1);
        let p = proposal(7, b"payload-7");
        let cert = certify(&set, &p);
        verify_finality(&set[3], &p.encode(), &cert, b"payload-7").unwrap();
    }

    #[test]
    fn payload_not_matching_the_proposal_is_rejected() {
        let set = validator_set(1);
        let p = proposal(7, b"payload-7");
        let cert = certify(&set, &p);
        let err = verify_finality(&set[0], &p.encode(), &cert, b"forged").unwrap_err();
        assert!(err.contains("do not match"), "{err}");
    }

    #[test]
    fn certificate_from_another_validator_set_is_rejected() {
        let ours = validator_set(1);
        let theirs = validator_set(2);
        let p = proposal(7, b"payload-7");
        let cert = certify(&theirs, &p);
        let err = verify_finality(&ours[0], &p.encode(), &cert, b"payload-7").unwrap_err();
        assert!(err.contains("does not verify"), "{err}");
    }

    #[test]
    fn certificate_for_another_proposal_is_rejected() {
        let set = validator_set(1);
        let signed = proposal(7, b"payload-7");
        let claimed = proposal(8, b"payload-7");
        let cert = certify(&set, &signed);
        let err = verify_finality(&set[0], &claimed.encode(), &cert, b"payload-7").unwrap_err();
        assert!(err.contains("does not verify"), "{err}");
    }

    #[test]
    fn malformed_certificate_is_rejected() {
        let set = validator_set(1);
        let p = proposal(7, b"payload-7");
        let err = verify_finality(&set[0], &p.encode(), &[0u8; 7], b"payload-7").unwrap_err();
        assert!(err.contains("certificate decode failed"), "{err}");
    }
}
