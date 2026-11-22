# Shadow devnet — commonware 2026.3.0 → 2026.10.0 (assessment, 2026-11-22)

Branch `shadow/commonware-2026.10` carries this assessment only; no code
has been swapped (115 differing files would not build unported).

## Why upgrade

Fleet pins vendored `2026.3.0`. Upstream releases since then contain
fixes that are literally our production failure modes:

| upstream | merged | our incident |
|---|---|---|
| #4956 — resolver echo suppression removed; certs into State | Oct 2 | certified-payload propagation luck |
| #5282 — never wait to send on state-sync/Monitor channels | Oct 17 | `Full(..)` mailbox drops under catch-up |
| #5325 — targeted fetches may reach secondary peers | Oct 20 | ancestry-fetch hole during mesh churn |

Our issue #5292 (bounded per-peer mailbox drops) is still OPEN upstream —
no maintainer reply after two nudges with production repro counts. The
2026.10 upgrade does not by itself close that one; keep tracking.

## New upstream mechanism that matters most: `certification_fetch`

`consensus/src/simplex/actors/voter/state.rs` (2026.10) now has an
in-protocol path for a blocked certification: when a candidate's parent
is uncertified, the voter **fetches the parent's certificate from any
peer** (not just the leader). This is the same failure class as our
certified-but-unreachable-parent wedges (#1–#4) that rc18/rc19 patches
handle at the application layer.

Open question to answer on the devnet before any fleet rollout: does
`certification_fetch` cover the **certified-but-unexecuted ancestry**
case (our `e6d8e9d` temporary-state verification) or only the
**missing-cert** case? If only the latter, rc19's app-layer admissibility
fix must be ported, not dropped.

## Upgrade surface (measured)

- `consensus/src`: **115 files differ** vs our vendored 2026.3.0.
- New upstream modules: `marshal/application/{build,gates,relay}.rs`,
  `aggregation/limits.rs`, `aggregation/mocks/scheme.rs`.
- Our vendored patch files to reconcile:
  - `marshal/application/verification_tasks.rs` (e6d8e9d — temporary-state
    verification for certified dead-variant wedge escape)
  - find_parent ordering — b0e76a1 already reverted to upstream behavior
    (upstream #5596 closed: certified-first ordering is load-bearing);
    nothing to port there.
- `storage`: one local patch (see docs/MAYO-AUDIT-PACKAGE.md §4).

## Plan

1. Re-vendor 2026.10.0 consensus + storage (tag tarball; workspace-field
   rewrite per cargo-vendor semantics). Choose 2026.10.0, not 2026.11.0 —
   it carries all three target fixes and has 3 weeks of upstream soak.
2. Port `verification_tasks.rs` onto the new `marshal/application/`
   structure (or drop if `certification_fetch` is proven to cover the
   dead-variant case on the devnet).
3. `cargo check -p commonware-consensus`, then full workspace build.
4. Devnet bring-up (devnet/docker-compose.yml): 5 seats, then the
   wedge-injection scenarios:
   - certified-but-unreachable parent (the wedge-4 fixture)
   - certified-but-unexecuted ancestry (dead variant)
   - catch-up under load (the #5292 mailbox-drop repro)
5. Soak ≥48h with finalization-lag + digest-parity checks before any
   fleet maintenance window is proposed.

## Non-goals

No live-fleet changes from this branch. The rc19 fleet rollout and the
shadow upgrade are independent tracks; rc19 stays the production line
until the shadow net proves the upgrade.
