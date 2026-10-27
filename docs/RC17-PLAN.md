# RC17 Plan — next consensus release scope

**Status: accumulating.** RC16 runs on the fleet today. This file is the
ordered scope for the next release; each item names the evidence that
put it here. Order = land sequence (instrument before changing).

## P0 — execution-path fix (INC-exec-lag-idx2-2026-10-27)

A perfectly healthy, fully-meshed seat (0 restarts, 7/7 sessions)
fell ~11k blocks behind in execution → `verify()`'s executed-parent
gate withheld votes → signer bit cycled out of certs 6× in one day.
Measured: 0.22 CPU core used, ~39% iowait, ~5.4k major faults/s —
**storage-bound, not CPU-bound**. Root cause candidate:

1. **Instrument first** — split `exec_ms` into `root_scan_ms` vs
   `apply_commit_ms` per finalized block and log it. Confirms the
   attribution before the bigger change lands.
2. **Incremental state-root tracker** — today `finalize_block` ends
   with `store_state_root()` → `compute_state_root()` → iterates
   EVERY consensus KV entry (O(state) per block, even 0-tx blocks;
   `app.rs` itself flags "fine at devnet scale"). Replace with an
   in-memory leaf index built once at startup + dirty-set updates
   per commit + bucketed rehash. Root bytes MUST be identical to
   the full scan (same leaf encoding, promote-odd, ordering —
   equivalence KAT vs `compute_state_root` + the
   `contracts/light-client` verifier). No protocol/wire change.
3. **Dirty-set capture** — wrap the commit writer's save/remove to
   expose mutated keys if the storage API doesn't already.
4. **Operator min-spec** (docs, interim): RAM ≥ ~2× state working
   set + NVMe-class IOPS; health metrics = `exec_ms` median +
   major-fault rate.

Acceptance test: posthuman's state-sync resync post-upgrade —
`exec_ms` ~10s → <1s, drain ≥1 blk/s.

## P1 — consensus protocol corrections (from upstream #5378)

Closed not-planned with fair attribution: our deployment diverged
from the reference model, not a library defect. The fixes are ours:

- **Epoch bump at reshare** — `CONSENSUS_EPOCH` is hardcoded
  `Epoch::new(0)`; every reshare has restarted with empty journals
  under the same identity+epoch (the exact configuration Brendan's
  Q4 probed). Epoch becomes the roster version: bump at every
  reshare → per-epoch journal partition + per-epoch subchannels
  when tracking the epoch-aware API upstream.
- **Duplicate-identity boot guard** — the retired G1 zombie sharing
  home-vb's ed25519 caused the likeliest first `block!`. Boot-time:
  data-dir/identity lock + local process check; ops gate stays in
  the runsheet (C0 audit gate is now permanent).
- **Temporary-state verify** — upstream-confirmed as the correct
  app-side fix for the certified-but-unfinalized parent wedge:
  execute candidate parents against temporary state, keep it
  separate from committed state, commit only on finalization.
  Biggest lift; may slip to RC18 — but the design is settled.
- **Blocked-peer observability** — until an upstream `blocked`
  gauge exists, monitor the symptom: TCP-connectable + no
  authenticated session + signer bit absent (what the zombie did
  to home-vb).
- **Slashing-evidence design pass** — the BLS aggregate proves
  quorum, not WHICH partials contributed; the hybrid bitmap
  records participation but isn't cryptographic proof. Needed
  before on-chain equivocation attribution (deferred design work).

## P2 — carried items

- Recovery epoch work, p2p hardening, certified-parent engine
  shape (once upstream's safety argument settles — track
  exocognosis's `Oracle::block` hard/soft split, may change the
  blocker API we consume).
- Read-only observer peer class (mesh blocks non-roster keys
  today; observers follow via public endpoints).
- Upstream watch: #5444 (metrics, open), #5394 (idle PR —
  Discussions note ~2 Nov if untouched).

## Explicitly NOT in RC17

- MAYO crypto changes — the exec-lag incident was storage, not PQ
  cost; `sig_cache` determinism is already landed and in audit
  scope. MAYO-audit items stay in `MAYO-AUDIT-PLAN.md`.
- Relay work (claw-relay) — separate track.
