# Finality-record backfill — repairing payload-only heights

## What it fixes

Proposal + certificate bytes are journaled only inside the
`Activity::Finalization` reporter path. A height that finalizes on the
fast-forward path (the node was mid-fetch when quorum formed) executes
its payload but never emits that activity — so the block's sidecar
record is **payload-only**: `QueryBlock` serves the payload with empty
`proposal_bytes`/`certificate_bytes`, and light-client verification for
that height cannot succeed. Observed on the vahana seat (~1/4 of
heights) under persistent slow-peer timing; see
`g1-ceremony/INCIDENT-2026-10-18-rc12.md`.

The backfill task repairs those records in the background by fetching
them from peer gRPCs — the same `layer.lightclient.v1 Query/Block`
endpoints used by state-sync.

## What it does (and does not do)

- Scans `payload present && certificate absent` heights: a historical
  watermark sweep, then a ~4k-height tail window to catch gaps as they
  form.
- Queries **every** configured donor for each missing height; a donor
  response counts only when `proposal_bytes` and `certificate_bytes`
  are non-empty AND `payload_bytes` byte-matches the locally executed
  payload (binds the record to the canonical chain). Prefers candidates
  backed by the most donors; a single-donor record is accepted with a
  warning.
- Writes via `set_block_finality` — atomic proposal + certificate +
  timestamp commit into `_`-prefixed **sidecar keys only**. No app_hash
  or execution impact; the repair is idempotent and safe to rerun.
- Bounded rate (~64 records/cycle ≈ 5.5k heights/hour) so donor gRPCs
  stay responsive.

It does **not** touch consensus, journals, or `_`-unprefixed state, and
it never overwrites an existing certificate record.

## Configuration

```toml
[state_sync]
peers = ["<donor-1>:9090", "<donor-2>:9090", "<donor-3>:9090"]
min_anchor_agree = 2

[finality_backfill]        # optional section
enabled = true             # default true when donors exist
dry_run = false            # true = scan + fetch + log, never write
```

The donor list is `state_sync.peers` — the sweep needs no separate peer
config. With no `[state_sync]` section it idles and logs one warning.

## Rollout procedure (vahana-class repair)

1. Deploy the binary containing the sweep. Set `dry_run = true` first.
2. Watch for `finality-backfill: dry-run — would repair` lines and the
   periodic `records repaired` summary (repaired count, queue depth,
   watermark). Confirm candidates are found and no
   `mismatches local payload` warnings appear.
3. Set `dry_run = false`, restart, and confirm the same heights now
   report `repaired` in logs and serve non-empty proposal/cert from
   `QueryBlock`.
4. Leave it running — the tail-window scan repairs new fast-forward
   gaps as they occur.

## Log lines

| Line | Meaning |
|---|---|
| `finality-backfill: started` (with `dry_run` flag) | task up, N donors |
| `records repaired { repaired, queued, watermark }` | batch committed (or dry-run candidates) |
| `donor record mismatches local payload` | donor served a non-canonical record — skipped, investigate if repeated |
| `single-donor record accepted` | only one donor had the record — accepted, flagged for audit |
| `no [state_sync] peers configured` | sweep idle; add donors to enable |
