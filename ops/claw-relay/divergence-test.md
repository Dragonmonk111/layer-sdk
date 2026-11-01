# J-Claw Relay — joint divergence test (v5)

Purpose: prove the pair stays *correct* under admission asymmetry —
each relay must apply its OWN admission gate to gossip ingress, and
divergent store state must be explainable, bounded, and non-cyclic.

Federation model (recap): pull-only. Each `--peer` task REQs all
channels on connect (history replay, ≤2000 events/REQ), EOSE, then the
sub stays live. Remote events pass the SAME `admit_event` gate as local
writes. Dedupe-by-id is the loop guard.

## Key semantics the test pins down

- cw4-derived npubs federate (both relays derive the same set).
- **Break-glass npubs (`allowlist.txt`) are operator-local by design.**
  A break-glass-signed event is admitted on the relay whose operator
  allowlisted it and REJECTED on all others → asymmetric stores are
  the CORRECT outcome for break-glass writes, not a bug. Consequence:
  the federated transcript is defined by cw4 members only; break-glass
  is for local/emergency writes and never federates.
- `allowlist.txt` is read once at boot; `member-bindings.json` re-reads
  every refresh — use allowlist.txt for the skew knob (restart rides
  the systemd install anyway).

## Test A — admission skew (primary)

Setup: pick a throwaway keypair `TEST` (64-hex, generate fresh — it
must NOT be a cw4 binding or in either allowlist).

A1. vahana: append `TEST` npub to `allowlist.txt`, restart relay
    (ride the systemd install). OVH: unchanged.
A2. TEST writes `E1` to vahana: `nip98-write.py --key-file test.sec
    --channel <governance-uuid> --content "divergence E1"` →
    expect local `200 {"accepted":true}`.
A3. OVH gossip pulls `E1` → remote ingress `admit_event` →
    `restricted: pubkey not allowlisted` → dropped.
    OVH-side evidence: `RUST_LOG=debug` would show
    `gossip event rejected … restricted:`; INFO-level evidence is
    simply E1 never appearing in OVH's store.
A4. Verify stores: `E1` present on vahana `/query`, ABSENT on OVH
    `/query`. Expected: asymmetric — TEST is break-glass-local.
A5. Reverse: OVH adds `TEST` to its allowlist, restarts. TEST writes
    `E2` to OVH. Verify `E2` on OVH, absent on vahana — AND `E1` still
    never federates backward (direction-independent local gating).
A6. Control: a cw4-bound member npub writes `E3` on vahana → expect it
    on BOTH stores within one gossip tick (~seconds).

## Test B — replay dedupe under divergent store

B1. After Test A, vahana's store has `E1` (break-glass) + `E3` (member).
B2. Force a gossip re-mate on OVH (`pkill -x claw-relay` on the peer
    task's conn is enough — or restart OVH relay).
B3. On reconnect OVH replays vahana's channel history: `E3` dedupes
    (already stored — no second stored copy, no rebroadcast storm),
    `E1` re-dropped at admission.
B4. Verify: OVH stored-event count for #governance increments by
    exactly zero for replayed ids; no `E1`; each replayed id stored once.

## Test C — refresh-window skew (observational, optional)

A mid-test cw4 membership change would create a ≤60s skew window where
the relays' effective sets differ transiently — same code path as
Test A (different admission on the same event). Not forced; note it as
covered by the mechanism if a real membership change lands during soak.

## Pass criteria

- TEST events NEVER cross an admission boundary in either direction.
- Member events cross in both directions.
- Store asymmetry is exactly and only the break-glass set.
- No event is stored twice anywhere after re-mate; no NOTICE/CLOSED
  storms; no reconnect thrash beyond the expected backoff lines.
- Both sides file: stored counts, the reject/accept log lines,
  and the query outputs for E1/E2/E3.
