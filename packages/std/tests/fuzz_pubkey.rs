//! Crafted-input fuzzing for the pubkey pack/parse/validate path.
//!
//! Deterministic seeded sweep (xorshift — no external fuzz infra). The
//! contract being tested:
//!
//!   - `from_hybrid_any_bytes` never panics on ANY byte string; a parsed key
//!     re-encodes to the identical wire bytes (canonical round-trip)
//!   - `validate_signature` never panics on ANY signature bytes, for every
//!     pubkey variant — malformed sigs return `Err(InvalidSignature)`
//!   - `account_id` never panics; unencodable keys return `Err`, NOT a
//!     silently-colliding sentinel address (regression: unwrap_or_default
//!     used to map every oversized secp key to one address)
//!   - `MayoVariant::from_byte` is total over all 256 byte values

use layer_std::{MayoVariant, PubKey};

/// xorshift64* — deterministic, cheap, good enough for input generation.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| (self.next() >> 32) as u8).collect()
    }
    /// Pick in [0, range)
    fn below(&mut self, range: usize) -> usize {
        (self.next() % range.max(1) as u64) as usize
    }
    /// Random length in [0, max), then that many random bytes.
    fn bytes_below(&mut self, max: usize) -> Vec<u8> {
        let n = self.below(max);
        self.bytes(n)
    }
}

const ALL_VARIANTS: [MayoVariant; 4] = [
    MayoVariant::Mayo1,
    MayoVariant::Mayo2,
    MayoVariant::Mayo3,
    MayoVariant::Mayo5,
];

/// Every byte value maps to a variant or None — never panics, no gaps in
/// the domain.
#[test]
fn fuzz_variant_from_byte_total() {
    for b in 0u16..=255 {
        let v = MayoVariant::from_byte(b as u8);
        match b {
            1 | 2 | 3 | 5 => assert!(v.is_some(), "variant byte {b} rejected"),
            _ => assert!(v.is_none(), "invalid variant byte {b} accepted"),
        }
    }
}

/// `from_hybrid_any_bytes` on arbitrary bytes: never panics; when it parses,
/// re-encoding produces the identical wire form (canonical).
#[test]
fn fuzz_from_hybrid_any_bytes_never_panics() {
    let mut rng = Rng::new(0xA11CE);
    for _ in 0..4096 {
        let len = rng.below(600);
        let input = rng.bytes(len);
        if let Some(pk) = PubKey::from_hybrid_any_bytes(&input) {
            let wire = pk
                .to_hybrid_any_bytes()
                .expect("parsed key must re-encode");
            assert_eq!(wire, input, "round-trip changed canonical wire form");
        }
    }
}

/// Directed boundary cases for the `secp_len` length-prefix: every prefix
/// value against undersized/oversized buffers — the slice `get(2..2+len)`
/// must bound, never read past.
#[test]
fn fuzz_secp_len_prefix_boundaries() {
    let mut rng = Rng::new(0xB0);
    for secp_len in [0usize, 1, 32, 33, 64, 65, 254, 255] {
        for variant in ALL_VARIANTS {
            for tail in [0usize, 1, 100, 300] {
                let mut input = vec![variant as u8, secp_len as u8];
                let body_len = secp_len.saturating_sub(1).min(tail);
                input.extend(rng.bytes(body_len));
                input.extend(rng.bytes(tail));
                // parse or reject — the only contract is "no panic"
                let _ = PubKey::from_hybrid_any_bytes(&input);
            }
        }
    }
    // Empty input and single-byte input.
    assert!(PubKey::from_hybrid_any_bytes(&[]).is_none());
    assert!(PubKey::from_hybrid_any_bytes(&[2]).is_none());
}

/// `validate_signature` on a hybrid key with random signature blobs of
/// every boundary length — 63/64/65 split point, huge trailing mayo half.
/// Must never panic; garbage must never verify.
#[test]
fn fuzz_validate_signature_garbage_sigs() {
    let mut rng = Rng::new(0x51);
    let msg_hash = rng.bytes(32);

    for variant in ALL_VARIANTS {
        let pk = PubKey::hybrid(
            rng.bytes(33),
            variant,
            // oversized mayo_pk is fine for the no-panic contract —
            // verify() rejects on length before touching the bytes
            rng.bytes(200),
        );
        for sig_len in [0usize, 1, 63, 64, 65, 127, 128, 1000, 20_000] {
            let sig = rng.bytes(sig_len);
            assert!(
                pk.validate_signature(&msg_hash, &sig).is_err(),
                "{variant:?}: garbage sig len {sig_len} verified"
            );
        }
        // Empty and random mayo_pk.
        let pk_empty = PubKey::hybrid(rng.bytes(33), variant, vec![]);
        assert!(pk_empty
            .validate_signature(&msg_hash, &rng.bytes(128))
            .is_err());
    }
}

/// `pack_hybrid_signature` + `validate_signature` on random inputs: packing
/// never panics regardless of half-lengths (a short secp half shifts the
/// 64-byte split and simply fails verify).
#[test]
fn fuzz_pack_and_validate_never_panics() {
    let mut rng = Rng::new(0xFACE);
    let msg_hash = rng.bytes(32);
    for variant in ALL_VARIANTS {
        let pk = PubKey::hybrid(rng.bytes(33), variant, rng.bytes(1500));
        for _ in 0..128 {
            let secp_half = rng.bytes_below(200);
            let mayo_half = rng.bytes_below(1200);
            let sig = PubKey::pack_hybrid_signature(&secp_half, &mayo_half);
            let _ = pk.validate_signature(&msg_hash, &sig);
        }
    }
}

/// `account_id` on every variant: never panics, deterministic.
/// Regression: a hybrid key whose secp part exceeds u8 (unencodable wire)
/// must return Err — previously `unwrap_or_default()` hashed an empty wire
/// and mapped EVERY such key to the same sentinel address.
#[test]
fn fuzz_account_id_no_panic_no_sentinel() {
    let mut rng = Rng::new(0xACC);
    for _ in 0..512 {
        let variant = match rng.below(3) {
            0 => PubKey::secp256k1(rng.bytes_below(64)),
            1 => PubKey::ed25519(rng.bytes_below(64)),
            _ => PubKey::hybrid(
                rng.bytes_below(40),
                ALL_VARIANTS[rng.below(4)],
                rng.bytes_below(6000),
            ),
        };
        // contract: returns Ok or Err, never panics
        let _ = variant.account_id();
    }

    // Sentinel regression: 300-byte secp part can't fit the u8 length prefix
    // → to_hybrid_any_bytes returns None → account_id must be Err, not a
    // shared sentinel hash.
    let bad = PubKey::hybrid(vec![0xAA; 300], MayoVariant::Mayo2, vec![1u8; 1400]);
    assert!(bad.account_id().is_err());
    let bad2 = PubKey::hybrid(vec![0xBB; 300], MayoVariant::Mayo2, vec![2u8; 1400]);
    assert!(bad2.account_id().is_err());
}

/// Ed25519 regression: the variant used to `todo!()` in both
/// `validate_signature` and `account_id` — a crafted tx carrying an ed25519
/// `Any` pubkey would panic the auth path.
#[test]
fn ed25519_no_longer_panics() {
    let pk = PubKey::ed25519(vec![3u8; 32]);
    assert!(pk.validate_signature(&[0u8; 32], &[0u8; 64]).is_err());
    assert!(pk.account_id().is_ok());
}
