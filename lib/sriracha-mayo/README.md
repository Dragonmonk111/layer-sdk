# `sriracha-mayo`

[![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/clabby/sriracha-mayo/ci.yaml?style=for-the-badge&label=CI)](https://github.com/clabby/sriracha-mayo/actions/workflows/ci.yaml)
[![Crates.io License](https://img.shields.io/crates/l/sriracha-mayo?style=for-the-badge)](https://crates.io/crates/sriracha-mayo)
[![Crates.io MSRV](https://img.shields.io/crates/msrv/sriracha-mayo?style=for-the-badge)](https://crates.io/crates/sriracha-mayo)
[![Crates.io Version](https://img.shields.io/crates/v/sriracha-mayo?style=for-the-badge)](https://crates.io/crates/sriracha-mayo)
[![docs.rs](https://img.shields.io/docsrs/sriracha-mayo?style=for-the-badge)](https://docs.rs/sriracha-mayo)

Safe Rust bindings for [MAYO-C](https://github.com/PQCMayo/MAYO-C).

The API is intentionally small:

```rust
use sriracha_mayo::{Mayo1, SecretKey};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;

let message = b"message";
let mut rng = ChaCha20Rng::from_seed([7; 32]);
let (public_key, secret_key) = SecretKey::<Mayo1>::random(&mut rng)?;
let signature = secret_key.sign(message)?;

assert!(signature.verify(&public_key, message));
# Ok::<(), sriracha_mayo::Error>(())
```

## Benchmarks

Criterion benchmarks from `cargo bench` on `aarch64-apple-darwin` (Apple M4 Max 2024)
with rustc `1.96.0` and `neon` enabled.

| Parameter set | Keygen | Sign | Verify | Verify 128 | Batch add + verify 128 |
| --- | ---: | ---: | ---: | ---: | ---: |
| MAYO-1 | 30.541 us | 107.59 us | 49.652 us | 6.0463 ms | 2.8870 ms |
| MAYO-2 | 29.344 us | 71.456 us | 17.376 us | 2.2614 ms | 746.01 us |
| MAYO-3 | 92.266 us | 259.03 us | 111.27 us | 14.358 ms | 8.2259 ms |
| MAYO-5 | 219.51 us | 575.34 us | 237.43 us | 30.534 ms | 17.256 ms |

## Feature Flags

The default feature set enables `std`. Build with `--no-default-features` for
`no_std` consumers; the crate still requires `alloc` for owned byte buffers and
batch verification.

Enable `serde` to derive `Serialize` and `Deserialize` for [`PublicKey`],
[`SecretKey`], and [`Signature`]. The encoded form is the compact MAYO-C key
encoding for keys and the detached signature bytes for signatures.

No backend feature is enabled by default. The `opt`, `avx2`, and `neon` features
select a MAYO-C backend explicitly when exactly one is enabled. If more than one
backend feature is enabled, `build.rs` falls back to platform auto-detection so
standard additive Cargo builds such as `--all-features` still work.

```sh
cargo build --no-default-features
cargo build --features serde
cargo build --features neon
```

## Parameter Sets

Use [`Mayo1`], [`Mayo2`], [`Mayo3`], or [`Mayo5`] as the type parameter for
[`PublicKey`], [`SecretKey`], and [`Signature`].

```rust
use sriracha_mayo::{Mayo3, SecretKey};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;

let mut rng = ChaCha20Rng::from_seed([7; 32]);
let (public_key, secret_key) = SecretKey::<Mayo3>::random(&mut rng)?;
let message = b"hello";
let signature = secret_key.sign(message)?;

assert!(signature.verify(&public_key, message));
# Ok::<(), sriracha_mayo::Error>(())
```

## Random Key Generation

Use [`SecretKey::random`] when key generation should draw randomness
from a caller-provided `rand_core` RNG.

```rust
use sriracha_mayo::{Mayo1, SecretKey};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;

let mut rng = ChaCha20Rng::from_seed([7; 32]);
let (public_key, secret_key) = SecretKey::<Mayo1>::random(&mut rng)?;

let message = b"message";
let signature = secret_key.sign(message)?;
assert!(signature.verify(&public_key, message));
# Ok::<(), sriracha_mayo::Error>(())
```

## Backend Selection

If no backend feature is selected, `build.rs` chooses `avx2` or `neon` when
the target supports it, and falls back to `opt`.

Use an explicit backend feature when you need a reproducible backend choice:

```sh
cargo build --features neon
```

## Submodule

This crate builds and statically links MAYO-C from the root-level `MAYO-C`
submodule. If the submodule is missing, initialize it before building:

```sh
git submodule update --init --recursive
```
