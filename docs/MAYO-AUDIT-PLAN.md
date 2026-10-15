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
  shipped — live at ~1.5M+ blocks.

## Scope the auditor gets

- `hybrid_scheme.rs` + its call sites in `app/slay3rd` (voter,
  reporter, cert production/verification)
- `packages/app/src/` — the state machine path that stores/uses certs
- `lib/commonware-*` vendored changes (we carry a patched
  `commonware-storage` — an auditor should see the diff)
- The KAT corpus + a written spec (`docs/PQ_PROTOCOL_AUTH.md` §4
  describes the intended wire format)

## Staging

1. Freeze a tagged commit for audit (rc11 or the audit-cut tag) —
  auditors work against a fixed hash.
2. Deliver the spec + threat model (what we assume about MAYO/BLS, what
  an attacker can do with one compromised share, replay story).
3. Field the report; fixes land on a `audit-fixes` branch; re-verify
  with the same KAT corpus + a live-chain regression run.

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
- Frozen audit tag
- Written threat-model section (extends `PQ_PROTOCOL_AUTH.md`)
