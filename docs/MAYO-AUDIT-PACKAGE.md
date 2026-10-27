# MAYO Audit Package — what the auditor receives

Companion to `MAYO-AUDIT-PLAN.md` (scope/staging). This file is the
frozen artifact list: exact pins, the KAT corpus, build instructions,
and the audit surface. Hand this + a tagged commit to the auditor.

## 1. Exact pins

| Component | Version | Source | Checksum |
|-----------|---------|--------|----------|
| `sriracha-mayo` | `0.0.4` | crates.io | `386585c8bc07644b649bdc81c47f91942bd0e6897f6d381268be6ac32d05368c` (both lockfiles) |
| `commonware-*` | `2026.3.0` | crates.io | see workspace `Cargo.lock` |
| `commonware-storage` | `2026.3.0` + local patch | vendored at `lib/commonware-storage/` | see §4 |

Pinned in: `app/slay3rd/Cargo.toml`, `tools/generate-testnet-keys/Cargo.toml`,
`tools/hybrid-sign/Cargo.toml`, `packages/junoclaw-mayo-verify/Cargo.toml`
— all `sriracha-mayo = "0.0.4"`, resolved identically by `Cargo.lock`.

## 2. KAT corpus

`packages/junoclaw-mayo-verify/tests/vectors/mayo{1,2,3,5}.txt`

- Produced by the reference C implementation via sriracha-mayo's
  `print_vectors` tests under `ChaCha20Rng` seed `[42;32]` —
  deterministic and regenerable.
- `tests/kat.rs` verifies our `junoclaw-mayo-verify` (the pure-Rust
  verifier used in the CosmWasm contract) against every vector, plus a
  deliberately-tampered signature that must reject.
- On-chain evidence: MAYO-2/3/5 attestations verified live through
  `jclaw-credential` — see `docs/ICS20_LOCAL_OSMOSIS_DEMO.md`.

To regenerate vectors: in `junoclaw-mayo-verify`, run the
`print_vectors` test with `--features sriracha-crosscheck` on a Unix
host with cmake (see §5).

## 3. Audit surface (files)

In order of review priority:

1. `app/slay3rd/src/hybrid_scheme.rs` — `HybridCertificate` encode/
   decode, bitmap↔index mapping, MAYO sign/verify calls, **`sig_cache`
   memoization** (signer determinism — production-found requirement,
   see PLAN §scope: replay/retransmit of an identical message MUST
   return the identical signature bytes or simplex reads it as
   equivocation).
2. Call sites: `app/slay3rd/src/node.rs` (cert assembly, vote/nullify
   paths, signer bitmap), `app/slay3rd/src/main.rs` (key load).
3. `packages/junoclaw-mayo-verify/src/lib.rs` — verification path used
   by the CosmWasm contract (`packages/cosmos/` integration).
4. `tools/generate-testnet-keys/src/main.rs` — MAYO2 keygen + the DKG
   flow that produces the BLS shares the hybrid certs sit under.
5. `lib/commonware-storage` — vendored diff, §4.

Spec: `docs/PQ_PROTOCOL_AUTH.md` §4 is the intended wire format.

## 4. Vendored commonware-storage diff

We carry `lib/commonware-storage` as a path dep. The only semantic
diff vs upstream `2026.3.0` is in `src/journal/segmented/manager.rs`
(`prune`): `Error::BlobMissing` from `context.remove()` is treated as
already-pruned (idempotent) instead of propagating fatally. Filed
upstream as #5266 — closed not-planned (shared-data-dir contract
violation on our side; the patch remains correct for our usage and
is retained deliberately).

Auditor attention: confirm no caller *relies* on prune failing when a
blob is absent, and that the in-memory `blobs` map stays consistent
when the early-return path is taken.

## 5. Build / verify instructions

```
# Authoritative build (what ships):
docker build -f docker/Dockerfile.slay3rd -t junoclaw-chain .
# — Rust 1.97 bookworm + clang + cmake; builds sriracha-mayo's MAYO-C.

# KAT + unit tests for the verifier:
cd packages/junoclaw-mayo-verify && cargo test            # pure-Rust
cargo test --features sriracha-crosscheck                 # Unix: vs C impl

# Node tests:
cargo test -p slay3rd        # hybrid cert encode/decode included
```

Windows builds exclude `sriracha-mayo` (unix-only); the Docker image is
the reference environment.

## 6. Threat model summary (extends `PQ_PROTOCOL_AUTH.md`)

- Attacker with one compromised BLS share cannot forge the classical
  threshold cert; attacker with one compromised MAYO key can inject a
  MAYO sig for their own bitmap bit only — cert validity still needs
  the BLS threshold.
- Replay: certs bind `namespace || proposal` (domain-separated); a
  cert from round N fails verification under any other proposal.
- The hybrid cert is additive — the BLS threshold alone is
  consensus-sufficient if MAYO verification were disabled; failure
  modes (missing/extra/dup sigs, oversized bitmap) are enumerated in
  `hybrid_scheme.rs` decode.
- **Determinism contract** — simplex equivocation detection compares
  signature bytes; randomized-signature schemes (MAYO, ML-DSA,
  SPHINCS+) MUST memoize per message (`sig_cache`) or an honest
  retransmit reads as conflicting evidence. Verified in production
  (#5378 discussion).
- **Cost side-channel** — execution throughput is the binding
  liveness constraint (INC-exec-lag-idx2-2026-10-27). Measured
  cause is `store_state_root()`'s O(state) scan per finalized
  block, NOT MAYO verify (0.22 core used, storage-bound). MAYO
  verify still sits on the same per-block cost line and should be
  included in the auditor's cost decomposition — but the incident
  shows the state-root scan dominates first. Liveness/min-spec
  axis, not a safety issue.
- **Attribution caveat** — the bitmap records participation; it is
  not cryptographic proof of which partials formed the threshold
  sig. Slashing-evidence design is a separate pass (deferred).
