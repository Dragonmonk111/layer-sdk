# MAYO Audit Plan — scope, staging, cost, funding

**Status: PLANNED.** This is the audit preparation document for the
post-quantum certificate layer. Trigger for engaging an auditor: after
G2 is stable and the DKG ceremony tool exists (the DKG changes how the
key material the certs rely on is produced).

## What is being audited

The hybrid certificate path in `app/slay3rd/src/hybrid_scheme.rs`:

- `HybridCertificate` — `[0x01][classical 48B BLS sig][signers bitmap][k × MAYO2 sig]`
- Encoding/decoding roundtrip — bitmap ordering, count consistency,
  malformed inputs (short buffer, count/len mismatch, tag confusion)
- The MAYO2 signature path — sign/verify against the pinned
  implementation, domain separation (`namespace || proposal`), and the
  signer-bitmap ↔ validator-index mapping
- The consensus integration — where the hybrid cert is assembled and
  where `vote`/`nullify` messages carry the PQ side (`voter` paths in
  `commonware-consensus` we wrap)
- **Signer determinism (production-found, #5378)** — MAYO draws a fresh
  salt per sign, so a bare signer emits different bytes for the same
  message; simplex's equivocation check compares vote bytes → reads an
  honest retransmit as conflicting → `block!(equivocator)`. Our
  `sig_cache` memoizes signatures per message to satisfy the scheme's
  determinism contract. Auditor must verify: (a) memoization covers
  EVERY sign path (replay, retransmit, restart-recovered votes),
  (b) cache eviction can't produce a second byte-distinct signature
  for an already-seen message, (c) two different signers can't
  collide in the cache key space.
- **Execution throughput vs block production rate (production-found,
  INC-exec-lag-idx2-2026-10-27)** — a fully-meshed seat fell ~10k+
  blocks behind in execution → `verify()` rejects proposals
  (executed-parent gate) → signer bit drops out of certs. Root cause
  is NOT the MAYO path: measured 0.22 CPU core used vs 39% iowait —
  the dominant per-block cost is `store_state_root()`'s O(state)
  full-KV iteration (called at the end of every `finalize_block`),
  which page-cache hides on large-RAM seats and thrashes on small
  ones (3.7 GiB seat: ~10s exec/block at ~0 tx/block). Fix is
  app-side (incremental state-root tracker, RC17) — listed here
  because it sits on the same execution hot path the auditor will
  read, and the per-block cost model (scan + MAYO verify + commit)
  is what the auditor should decompose.
- Failure modes: missing sig for a set bit, extra sigs, dup indices,
  oversized bitmap, sig cache overflow eviction during replay

Out of scope (already-covered or separate): the BLS threshold cert
itself (Commonware, externally reviewed), MAYO primitive internals
(reference KATs exist; we're auditing *our usage*, not the scheme).

## What's already tested

- MAYO-2/3/5 attestations verified on-chain through the
  `jclaw-credential` contract on devnet (KAT vectors, including a
  deliberately-tampered signature that correctly rejected) — devnet
  record in `docs/ICS20_LOCAL_OSMOSIS_DEMO.md`.
- Hybrid certs carried on every `junoclaw-g1` block since the feature
  shipped — live at ~3.2M+ blocks on the current roster (hybrid
  envelopes began at the R2 resumption, height 2,631,740).
- **Independent verification in flight** — public bundle
  `junoclaw-verification-bundle-v1` (roster pubkeys, group key, cert
  layout, protos) given to an external operator to reproduce cert
  verification against `junoclaw-mayo-verify`; first external verifier
  result pending.
- **Signer-bitmap caveat (for slashing design)** — the aggregate BLS
  signature proves quorum, not WHICH partials contributed; the hybrid
  bitmap records participation but is not cryptographic proof of who
  signed. A slashing-evidence design pass is needed before on-chain
  equivocation attribution (see `BLS_LIGHT_CLIENT_SPEC.md` §on
  equivocation mapping).

## Scope the auditor gets

- `hybrid_scheme.rs` + its call sites in `app/slay3rd` (voter,
  reporter, cert production/verification)
- `packages/app/src/` — the state machine path that stores/uses certs
- `lib/commonware-*` vendored changes (we carry a patched
  `commonware-storage` — an auditor should see the diff)
- The KAT corpus + a written spec (`docs/PQ_PROTOCOL_AUTH.md` §4
  describes the intended wire format)

## Staging

1. Freeze a tagged commit for audit (rc16-current or the RC17 cut —
  rc16 is what runs on the fleet today; the audit artifact should be
  the production binary, not an older tag).
2. Deliver the spec + threat model (what we assume about MAYO/BLS, what
  an attacker can do with one compromised share, replay story) PLUS
  the two production findings above (signer determinism, PQ verify
  cost) — they are already-found issues the audit should confirm
  fixed, not discover fresh.
3. Field the report; fixes land on a `audit-fixes` branch; re-verify
  with the same KAT corpus + a live-chain regression run.

## Upstream context (as of 2026-10-27)

- `commonware-* 2026.3.0` pinned; vendored `commonware-storage` patch
  reported upstream (#5266, closed not-planned — contract violation
  acknowledged, patch remains correct for our usage).
- #5378 (4h peer-block isolation) closed not-planned after full
  disclosure — attributed to our deployment (duplicate identity,
  epoch-0 reuse, wiped journals, verify gate), NOT the library.
  Determinism contract confirmed upstream: scheme signers must be
  deterministic — our `sig_cache` is the conformant fix.
- #5444 (mailbox-drop metrics) open; `Oracle::block` hard/soft split
  follow-up in flight (upstream maintainer) — may change the blocker
  API surface we consume; RC17 should track it.
- Audit relevance: the consensus-interface boundary is now well
  understood and documented publicly in the issue threads.

## Cost / funding

External Rust+BFT+PQ review for this scope typically runs a few
weeks of auditor time. The funding trigger conversation starts once
the DKG tooling exists — the audited artifact should be the final
mainnet-facing path, not a pre-DKG interim one. Sizing the ask depends
on which auditor (Rust/security firms vs. the Commonware team's own
review channel — the vendored-crate diff argues for the latter for the
storage patch specifically).

## Dependencies

- DKG tool (dealer-free share dealing) — in flight
- Frozen audit tag (rc16-current or RC17 cut)
- Written threat-model section (extends `PQ_PROTOCOL_AUTH.md`)
- External verification result (alxvoy reproduction of cert verify
  against bundle v1) — strengthens the "independently verified" claim
  the audit rests on
- RC17 consensus items that touch the cert path (epoch bump at
  reshare, temp-state verify) — sequence: freeze audit AFTER these
  land so the audited artifact is the final shape
