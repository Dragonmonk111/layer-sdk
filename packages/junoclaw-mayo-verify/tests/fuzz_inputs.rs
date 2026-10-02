//! Crafted-input fuzzing for the MAYO verifier.
//!
//! Deterministic (seeded xorshift — no external fuzz infra needed) sweep of
//! malformed inputs against `verify::<P>` for every parameter set. The
//! contract being tested:
//!
//!   - never panics, for ANY input lengths/contents
//!   - wrong-length inputs → `Err(Error::InvalidLength)`
//!   - correct-length garbage → `Ok(false)` (never `Ok(true)`)
//!
//! Run the long soak with: `cargo test -p junoclaw-mayo-verify --test fuzz_inputs -- --ignored`

use junoclaw_mayo_verify::{verify, Mayo1, Mayo2, Mayo3, Mayo5, ParameterSet};

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

/// Exercise one parameter set: assert the error/panic contract on
/// `iters` random inputs. `iters` of 0 for large param sets keeps CI fast.
fn fuzz_param<P: ParameterSet>(iters: usize) {
    let mut rng = Rng::new(P::SIG_BYTES as u64 ^ 0x5EED);

    // Boundary sweep: every length in [len-2, len+2] for sig and pk.
    // Wrong lengths must be Err(InvalidLength), never panic.
    for delta in -2i64..=2 {
        for (sig_len, pk_len, should_err) in [
            (P::SIG_BYTES as i64 + delta, P::PK_BYTES as i64, delta != 0),
            (P::SIG_BYTES as i64, P::PK_BYTES as i64 + delta, delta != 0),
        ] {
            if sig_len < 0 || pk_len < 0 {
                continue;
            }
            let sig = rng.bytes(sig_len as usize);
            let pk = rng.bytes(pk_len as usize);
            let msg = rng.bytes_below(256);
            match verify::<P>(&msg, &sig, &pk) {
                Err(_) if should_err => {}
                Ok(v) if !should_err => assert!(!v, "{}: garbage verified", P::NAME),
                Err(e) => panic!("{}: right-length input errored: {e}", P::NAME),
                Ok(v) => panic!("{}: wrong-length input returned Ok({v})", P::NAME),
            }
        }
    }

    // Degenerate contents at correct lengths.
    for fill in [0x00u8, 0xFF, 0x55, 0xAA] {
        let sig = vec![fill; P::SIG_BYTES];
        let pk = vec![fill; P::PK_BYTES];
        assert!(
            !verify::<P>(b"msg", &sig, &pk).expect("must not error at right length"),
            "{}: uniform-fill sig/pk verified",
            P::NAME
        );
    }

    // Empty and huge messages must not panic.
    let sig = rng.bytes(P::SIG_BYTES);
    let pk = rng.bytes(P::PK_BYTES);
    let _ = verify::<P>(&[], &sig, &pk).unwrap();
    let big_msg = rng.bytes(1 << 20); // 1 MiB — hash input, should be fine
    let _ = verify::<P>(&big_msg, &sig, &pk).unwrap();

    // Random correct-length garbage.
    for _ in 0..iters {
        let sig = rng.bytes(P::SIG_BYTES);
        let pk = rng.bytes(P::PK_BYTES);
        let msg = rng.bytes_below(1024);
        assert!(
            !verify::<P>(&msg, &sig, &pk).unwrap(),
            "{}: random input verified",
            P::NAME
        );
    }
}

#[test]
fn fuzz_mayo1() {
    fuzz_param::<Mayo1>(256);
}

#[test]
fn fuzz_mayo2() {
    fuzz_param::<Mayo2>(256);
}

#[test]
fn fuzz_mayo3() {
    fuzz_param::<Mayo3>(128);
}

#[test]
fn fuzz_mayo5() {
    // MAYO-5 verify is the heaviest — keep CI iterations modest.
    fuzz_param::<Mayo5>(64);
}

/// Long soak — run explicitly: --ignored
#[test]
#[ignore]
fn fuzz_soak_all_variants() {
    fuzz_param::<Mayo1>(4096);
    fuzz_param::<Mayo2>(4096);
    fuzz_param::<Mayo3>(2048);
    fuzz_param::<Mayo5>(512);
}
