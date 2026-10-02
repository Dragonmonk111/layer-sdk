//! Hybrid (classical + post-quantum) consensus signing scheme.
//!
//! Implements `commonware_cryptography::certificate::Scheme` by wrapping the
//! existing BLS12-381 threshold scheme and attaching a MAYO2 signature to
//! every attestation. Both halves sign the exact same bytes the classical
//! half signs (`namespace || message`), so a valid classical vote is
//! meaningless without its MAYO counterpart — classical and PQ security are
//! AND'd together (docs/PQ_PROTOCOL_AUTH.md §3-§4).
//!
//! Wire formats:
//!
//! ```text
//! Attestation signature (fixed-size):
//!   hybrid_sig = [bls_partial_sig: ClassicalSignature::SIZE][mayo2_sig: 186]
//!
//! Certificate (variable-size, version-tagged per §5 envelope versioning):
//!   HybridCertificate = [tag: 0x01][classical_cert][signer_bitmap][mayo_sig * k]
//!   sigs are ordered by ascending signer index (same as the bitmap).
//! ```
//!
//! Platform note: MAYO *signing* requires `sriracha-mayo` (wraps the MAYO-C
//! reference implementation, Unix-only). On non-Unix hosts `sign()` returns
//! `None` — the node still verifies hybrid attestations and certificates via
//! the pure-Rust `junoclaw-mayo-verify` crate. Devnet validators run in
//! Linux containers, so this is a build-host limitation only.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::sync::{Arc, Mutex};

use bytes::{Buf, BufMut};
use commonware_codec::{
    types::lazy::Lazy, EncodeSize, Error as CodecError, FixedSize, Read, Write,
};
use commonware_consensus::simplex::scheme::bls12381_threshold::standard::Scheme as BlsScheme;
use commonware_consensus::simplex::scheme::Namespace as SchemeNamespace;
use commonware_consensus::simplex::types::Subject as SimplexSubject;
use commonware_cryptography::bls12381::primitives::variant::MinSig;
use commonware_cryptography::certificate::{
    Attestation, Scheme as CertificateScheme, Signers, Subject, Verification,
};
use commonware_cryptography::{ed25519, Digest};
use commonware_parallel::Strategy;
use commonware_utils::{ordered::Set, Faults, Participant};
use junoclaw_mayo_verify::{Mayo2, ParameterSet};
use rand_core::CryptoRngCore;

/// The concrete classical scheme the hybrid scheme wraps.
pub type InnerScheme = BlsScheme<ed25519::PublicKey, MinSig>;

/// Certificate tag: hybrid certificates are prefixed `0x01` on the wire
/// (legacy BLS-only certificates would use `0x00`; see PQ_PROTOCOL_AUTH.md §5).
pub const CERT_TAG_HYBRID: u8 = 0x01;

/// MAYO2 signature size — fixed, which is what makes `HybridSignature`
/// satisfy the `CodecFixed` bound required by `certificate::Scheme`.
const MAYO_SIG_BYTES: usize = Mayo2::SIG_BYTES;

/// Classical (BLS12-381 threshold partial) signature type.
type ClassicalSignature = <InnerScheme as CertificateScheme>::Signature;

/// Classical certificate type (threshold-recovered group signature).
type ClassicalCertificate = <InnerScheme as CertificateScheme>::Certificate;

/// Builds the bytes both halves sign: `subject.namespace(derived) || subject.message()`.
///
/// This must match what the inner BLS scheme signs — the classical threshold
/// scheme calls `ops::sign_message`/`verify_message` over exactly these bytes.
fn namespaced_message<D: Digest>(namespace: &SchemeNamespace, subject: &SimplexSubject<'_, D>) -> Vec<u8> {
    let ns: &[u8] = subject.namespace(namespace);
    let msg = subject.message();
    let mut out = Vec::with_capacity(ns.len() + msg.len());
    out.extend_from_slice(ns);
    out.extend_from_slice(&msg);
    out
}

/// Signs `msg` with MAYO2 using `sriracha-mayo` (Unix-only C-backed signing).
/// The secret key is re-expanded from seed per call: `sriracha_mayo::SecretKey`
/// holds C allocations and is not `Send`/`Sync`, so it cannot be stored in the
/// scheme (which must be `Send + Sync` for the consensus actors).
#[cfg(unix)]
fn sign_mayo(seed: &[u8], msg: &[u8]) -> Option<[u8; MAYO_SIG_BYTES]> {
    let (_pk, sk) = sriracha_mayo::SecretKey::<sriracha_mayo::Mayo2>::from_seed(seed).ok()?;
    let sig = sk.sign(msg).ok()?;
    let bytes: &[u8] = sig.as_ref();
    bytes.try_into().ok()
}

/// MAYO signing is unavailable on non-Unix hosts (see module docs).
/// Verifier-only nodes still function; a validator on such a host simply
/// cannot produce votes (sign returns None up front).
#[cfg(not(unix))]
fn sign_mayo(_seed: &[u8], _msg: &[u8]) -> Option<[u8; MAYO_SIG_BYTES]> {
    None
}

/// Verifies a MAYO2 signature via the pure-Rust `junoclaw-mayo-verify` crate —
/// works on every platform.
fn mayo_verify(msg: &[u8], sig: &[u8], pk: &[u8]) -> bool {
    junoclaw_mayo_verify::verify::<Mayo2>(msg, sig, pk).unwrap_or(false)
}

/// Per-vote hybrid signature: classical BLS partial + MAYO2 signature,
/// both over the same `namespace || message` bytes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HybridSignature {
    /// BLS12-381 partial signature from the signer's threshold share.
    pub classical: ClassicalSignature,
    /// MAYO2 signature over the identical message bytes.
    pub pq: [u8; MAYO_SIG_BYTES],
}

impl FixedSize for HybridSignature {
    const SIZE: usize = ClassicalSignature::SIZE + MAYO_SIG_BYTES;
}

impl Write for HybridSignature {
    fn write(&self, buf: &mut impl BufMut) {
        self.classical.write(buf);
        buf.put_slice(&self.pq);
    }
}

impl Read for HybridSignature {
    type Cfg = ();

    fn read_cfg(buf: &mut impl Buf, _: &()) -> Result<Self, CodecError> {
        Ok(Self {
            classical: ClassicalSignature::read_cfg(buf, &())?,
            pq: <[u8; MAYO_SIG_BYTES]>::read_cfg(buf, &())?,
        })
    }
}

/// Hybrid certificate: the recovered classical threshold certificate plus a
/// bitmap-addressed set of MAYO2 signatures, one per set bit (PqCert in
/// PQ_PROTOCOL_AUTH.md §4).
///
/// Encoding is `[0x01][classical][signers][sig_0 .. sig_k)` where `k` is the
/// bitmap's population count and sigs are ordered by ascending signer index.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HybridCertificate {
    /// Classical threshold certificate, unchanged.
    pub classical: ClassicalCertificate,
    /// Which participants' MAYO keys signed.
    pub signers: Signers,
    /// MAYO2 signatures, one per set bit of `signers`, in ascending signer order.
    pub pq_sigs: Vec<[u8; MAYO_SIG_BYTES]>,
}

impl Write for HybridCertificate {
    fn write(&self, buf: &mut impl BufMut) {
        CERT_TAG_HYBRID.write(buf);
        self.classical.write(buf);
        self.signers.write(buf);
        for sig in &self.pq_sigs {
            buf.put_slice(sig);
        }
    }
}

impl EncodeSize for HybridCertificate {
    fn encode_size(&self) -> usize {
        1 + self.classical.encode_size()
            + self.signers.encode_size()
            + self.pq_sigs.len() * MAYO_SIG_BYTES
    }
}

impl Read for HybridCertificate {
    /// `usize` = maximum participant count (bounds the signer bitmap decode).
    type Cfg = usize;

    fn read_cfg(buf: &mut impl Buf, max_participants: &usize) -> Result<Self, CodecError> {
        let tag = u8::read_cfg(buf, &())?;
        if tag != CERT_TAG_HYBRID {
            return Err(CodecError::Invalid(
                "HybridCertificate",
                "expected 0x01 hybrid tag",
            ));
        }
        let classical = ClassicalCertificate::read_cfg(buf, &())?;
        let signers = Signers::read_cfg(buf, max_participants)?;
        let mut pq_sigs = Vec::with_capacity(signers.count());
        for _ in 0..signers.count() {
            pq_sigs.push(<[u8; MAYO_SIG_BYTES]>::read_cfg(buf, &())?);
        }
        Ok(Self {
            classical,
            signers,
            pq_sigs,
        })
    }
}

/// Hybrid consensus scheme: wraps the BLS12-381 threshold scheme and binds a
/// MAYO2 signature to every attestation. Certificates carry the classical
/// threshold signature plus a bitmap of per-signer MAYO signatures.
///
/// Validity rule (PQ_PROTOCOL_AUTH.md §4):
///   valid(cert) ≡ bls_threshold_verify(classical) AND
///                 count(valid_mayo_sigs) ≥ pq_quorum
/// with `pq_quorum` set equal to the BLS `threshold_required` in Phase 2.
/// Cap on cached signatures — bounds memory (a handful of subjects per view).
/// On overflow the map is cleared; a cleared entry simply re-signs on demand.
const SIG_CACHE_CAP: usize = 8192;

pub struct HybridScheme {
    /// Classical threshold scheme — owns the participant set and BLS shares.
    inner: InnerScheme,
    /// Derived simplex namespaces (notarize/nullify/finalize) — must equal
    /// the inner scheme's derived namespaces so both halves sign the same bytes.
    namespace: SchemeNamespace,
    /// MAYO2 verification keys indexed by participant index (same order as the
    /// ordered Ed25519 participant set).
    mayo_pks: Vec<Vec<u8>>,
    /// This validator's MAYO seed (from_seed input), absent for verifier-only
    /// instances or non-Unix builds that cannot sign.
    mayo_seed: Option<Vec<u8>>,
    /// Required number of valid MAYO signatures in a certificate.
    pq_quorum: usize,
    /// Memoized MAYO signatures keyed by namespaced message bytes.
    ///
    /// MAYO-C draws a fresh random salt per `sign` call, so signing the same
    /// vote twice yields different bytes. Simplex compares votes byte-for-byte
    /// to detect equivocation — two byte-different artifacts for one subject
    /// read as `conflicting` and the sender gets blocked, which partitions the
    /// validator set. Re-signing identical subjects happens legitimately
    /// (journal replay, retransmit). The cache makes the wrapper deterministic
    /// per message: identical bytes get the identical signature back, while
    /// distinct messages still receive fresh random salts. Salt reuse across
    /// *different* messages is the security hazard; reuse over the *same*
    /// bytes is equivalent to emitting the same signature twice — safe.
    ///
    /// Shared across clones via `Arc` — all clones must hit one cache.
    sig_cache: Arc<Mutex<HashMap<Vec<u8>, [u8; MAYO_SIG_BYTES]>>>,
}

impl HybridScheme {
    /// Wraps a constructed classical scheme.
    ///
    /// `mayo_pks[i]` must be the MAYO2 public key of the validator at
    /// participant index `i` (i.e. in the same order as
    /// `participants()` — the binary-sorted Ed25519 key order).
    ///
    /// Returns `None` if `mayo_pks` does not have exactly one entry per
    /// participant or any key has the wrong size.
    pub fn new(
        inner: InnerScheme,
        base_namespace: &[u8],
        mayo_pks: Vec<Vec<u8>>,
        mayo_seed: Option<Vec<u8>>,
        pq_quorum: usize,
    ) -> Option<Self> {
        let n = inner.participants().iter().count();
        if mayo_pks.len() != n {
            return None;
        }
        if mayo_pks.iter().any(|pk| pk.len() != Mayo2::PK_BYTES) {
            return None;
        }
        if pq_quorum == 0 || pq_quorum > n {
            return None;
        }
        Some(Self {
            inner,
            namespace: SchemeNamespace::new(base_namespace),
            mayo_pks,
            mayo_seed,
            pq_quorum,
            sig_cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }
}

impl Clone for HybridScheme {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            namespace: self.namespace.clone(),
            mayo_pks: self.mayo_pks.clone(),
            mayo_seed: self.mayo_seed.clone(),
            pq_quorum: self.pq_quorum,
            sig_cache: Arc::clone(&self.sig_cache),
        }
    }
}

impl fmt::Debug for HybridScheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HybridScheme")
            .field("participants", &self.mayo_pks.len())
            .field("pq_quorum", &self.pq_quorum)
            .field("has_mayo_seed", &self.mayo_seed.is_some())
            .finish()
    }
}

impl CertificateScheme for HybridScheme {
    type Subject<'a, D: Digest> = SimplexSubject<'a, D>;
    type PublicKey = ed25519::PublicKey;
    type Signature = HybridSignature;
    type Certificate = HybridCertificate;

    fn me(&self) -> Option<Participant> {
        // Verifier-only if we have no MAYO material to sign with.
        if self.mayo_seed.is_none() {
            return None;
        }
        self.inner.me()
    }

    fn participants(&self) -> &Set<Self::PublicKey> {
        self.inner.participants()
    }

    fn sign<D: Digest>(&self, subject: Self::Subject<'_, D>) -> Option<Attestation<Self>> {
        let seed = self.mayo_seed.as_ref()?;
        // MAYO signs the identical namespaced bytes the BLS half signs.
        let msg = namespaced_message(&self.namespace, &subject);
        let pq = {
            let mut cache = self
                .sig_cache
                .lock()
                .expect("sig cache mutex poisoned");
            match cache.get(&msg) {
                Some(sig) => *sig,
                None => {
                    let sig = sign_mayo(seed, &msg)?;
                    if cache.len() >= SIG_CACHE_CAP {
                        cache.clear();
                    }
                    cache.insert(msg, sig);
                    sig
                }
            }
        };
        let attestation = self.inner.sign(subject)?;
        let classical = attestation.signature.get()?.clone();
        Some(Attestation {
            signer: attestation.signer,
            signature: Lazy::new(HybridSignature { classical, pq }),
        })
    }

    fn verify_attestation<R, D>(
        &self,
        rng: &mut R,
        subject: Self::Subject<'_, D>,
        attestation: &Attestation<Self>,
        strategy: &impl Strategy,
    ) -> bool
    where
        R: CryptoRngCore,
        D: Digest,
    {
        let Some(sig) = attestation.signature.get() else {
            return false;
        };

        // Classical half first (cheap batchable verify upstream).
        let classical_att = Attestation::<InnerScheme> {
            signer: attestation.signer,
            signature: Lazy::new(sig.classical.clone()),
        };
        if !self
            .inner
            .verify_attestation::<R, D>(rng, subject.clone(), &classical_att, strategy)
        {
            return false;
        }

        // PQ half: MAYO2 sig against this signer's registered key.
        let Some(mayo_pk) = self.mayo_pks.get(usize::from(attestation.signer)) else {
            return false;
        };
        let msg = namespaced_message(&self.namespace, &subject);
        mayo_verify(&msg, &sig.pq, mayo_pk)
    }

    fn verify_attestations<R, D, I>(
        &self,
        rng: &mut R,
        subject: Self::Subject<'_, D>,
        attestations: I,
        strategy: &impl Strategy,
    ) -> Verification<Self>
    where
        R: CryptoRngCore,
        D: Digest,
        I: IntoIterator<Item = Attestation<Self>>,
        I::IntoIter: Send,
    {
        // Split hybrid attestations into classical ones so the inner scheme's
        // batch verification applies to the BLS halves.
        let mut inner_atts = Vec::new();
        let mut orig: BTreeMap<Participant, Attestation<Self>> = BTreeMap::new();
        let mut invalid: Vec<Participant> = Vec::new();
        for att in attestations {
            match att.signature.get() {
                Some(sig) => {
                    inner_atts.push(Attestation::<InnerScheme> {
                        signer: att.signer,
                        signature: Lazy::new(sig.classical.clone()),
                    });
                    orig.insert(att.signer, att);
                }
                None => invalid.push(att.signer),
            }
        }

        let inner_res = self.inner.verify_attestations::<R, D, _>(
            rng,
            subject.clone(),
            inner_atts,
            strategy,
        );
        invalid.extend(inner_res.invalid.iter().copied());

        // MAYO-verify each classically-valid attestation.
        let msg = namespaced_message(&self.namespace, &subject);
        let mut verified = Vec::new();
        for inner_att in inner_res.verified {
            let Some(hybrid) = orig.remove(&inner_att.signer) else {
                continue;
            };
            let sig = match hybrid.signature.get() {
                Some(s) => s,
                None => {
                    invalid.push(inner_att.signer);
                    continue;
                }
            };
            match self.mayo_pks.get(usize::from(inner_att.signer)) {
                Some(pk) if mayo_verify(&msg, &sig.pq, pk) => verified.push(hybrid),
                _ => invalid.push(inner_att.signer),
            }
        }
        // Any leftovers not returned by the inner batch are invalid.
        invalid.extend(orig.keys().copied());

        Verification::new(verified, invalid)
    }

    fn assemble<I, M>(
        &self,
        attestations: I,
        strategy: &impl Strategy,
    ) -> Option<Self::Certificate>
    where
        I: IntoIterator<Item = Attestation<Self>>,
        I::IntoIter: Send,
        M: Faults,
    {
        let n = self.mayo_pks.len();
        let mut inner_atts = Vec::new();
        let mut pq: BTreeMap<Participant, [u8; MAYO_SIG_BYTES]> = BTreeMap::new();
        let mut seen = BTreeSet::new();
        for att in attestations {
            let signer = att.signer;
            // Guard Signers::from (panics on duplicates/out-of-range).
            if usize::from(signer) >= n || !seen.insert(signer) {
                continue;
            }
            let sig = att.signature.get()?.clone();
            inner_atts.push(Attestation::<InnerScheme> {
                signer,
                signature: Lazy::new(sig.classical),
            });
            pq.insert(signer, sig.pq);
        }

        // The classical assemble enforces its own quorum; attestations were
        // already verify_attestation'd by the voter, so every collected MAYO
        // sig is valid and rides along in signer-sorted order.
        let classical = self.inner.assemble::<_, M>(inner_atts, strategy)?;
        Some(HybridCertificate {
            classical,
            signers: Signers::from(n, pq.keys().copied()),
            pq_sigs: pq.values().copied().collect(),
        })
    }

    fn verify_certificate<R, D, M>(
        &self,
        rng: &mut R,
        subject: Self::Subject<'_, D>,
        certificate: &Self::Certificate,
        strategy: &impl Strategy,
    ) -> bool
    where
        R: CryptoRngCore,
        D: Digest,
        M: Faults,
    {
        // Classical half.
        if !self.inner.verify_certificate::<_, _, M>(
            rng,
            subject.clone(),
            &certificate.classical,
            strategy,
        ) {
            return false;
        }

        // Bitmap must cover exactly the participant set and be consistent
        // with the sig list.
        if certificate.signers.len() != self.mayo_pks.len() {
            return false;
        }
        if certificate.pq_sigs.len() != certificate.signers.count() {
            return false;
        }

        // PQ half: independent quorum of valid MAYO signatures over the same
        // namespaced message.
        let msg = namespaced_message(&self.namespace, &subject);
        let mut valid = 0usize;
        for (signer, sig) in certificate.signers.iter().zip(&certificate.pq_sigs) {
            match self.mayo_pks.get(usize::from(signer)) {
                Some(pk) if mayo_verify(&msg, sig, pk) => valid += 1,
                _ => continue,
            }
        }
        valid >= self.pq_quorum
    }

    fn is_attributable() -> bool {
        InnerScheme::is_attributable()
    }

    fn is_batchable() -> bool {
        InnerScheme::is_batchable()
    }

    fn certificate_codec_config(&self) -> <Self::Certificate as Read>::Cfg {
        self.mayo_pks.len()
    }

    fn certificate_codec_config_unbounded() -> <Self::Certificate as Read>::Cfg {
        usize::MAX
    }
}
