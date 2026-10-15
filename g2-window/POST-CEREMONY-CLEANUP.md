# Post-Ceremony Material Hygiene — Checkpoint

**DO NOT SKIP. Run through this checklist after each ceremony window
completes and the chain is finalizing on the new set.**

Standard ceremony hygiene: plaintext sensitive material exists only for
the duration of the ceremony. Everything below is inventoried so deletion
is deterministic — check each box, don't rely on memory.

## On each operator's box (operator's own cleanup)

| Material | Where | Delete when |
|---|---|---|
| `dealer.seed` (32B) | `dkg-out/` on each dealer host | after `dkg-log` emitted its log |
| `dealing-priv-*-for-*.json` | delivered per-player | after `dkg-finalize` succeeds |
| `bls-share.json` (standalone file) | `ceremony/` working dir | after `finalize` merges it into `keys.json` |
| old `keys.json` backups | operator's choice | only after new set is finalizing AND share verified |
| `data/consensus.g2-bak` (old journal) | per-seat `/data` | after window confirmed stable (~24h) — keep until then, it's the rollback path |

## On the coordinator (us)

| Material | Where | Delete when |
|---|---|---|
| `g1-keys/*.json` (synthesis copies) | `g2-window/g1-keys/` | after `observe.sh` asserts key preserved |
| `ceremony/priv-<seat>/` dirs | `g2-window/ceremony/` | after all seats confirm finalize |
| `ceremony/bls-share-<seat>.json` | `g2-window/ceremony/` | after each seat's finalize confirmed |
| collected `acks/`, `logs/` | `g2-window/ceremony/` | keep — public/signed material, fine to keep for audit |
| `ceremony/dkg-output-*.json`, `shared.json`, `dkg-info.json` | `g2-window/ceremony/` | **keep — public, needed for next reshare round** |
| rehearsal key copies | `/tmp/*`, any stray dirs | immediately after rehearsal |

## Never delete

- `ceremony/dkg-output-<n>.json` — the `--previous` input for the NEXT
  reshare. Losing it doesn't break the chain but forces a fresh deal
  (new group key → IBC trust-root rotation).
- `shared.json`, `dkg-info.json`, dealer logs — public audit trail.
- Operators' `keys.json` — operator-owned, never on the coordinator.

## Sweep commands (run at cleanup time)

```bash
# coordinator
shred -u g2-window/g1-keys/*.json 2>/dev/null || rm -f g2-window/g1-keys/*.json
rm -rf g2-window/ceremony/priv-* g2-window/ceremony/bls-share-*.json

# each operator (they run it — remind in ops channel)
shred -u dkg-out/dealer.seed
rm -rf priv/ acks/ bls-share.json   # after finalize confirmed
```

## Standing rule

If you find key material (`keys.json`, `dealer.seed`, `bls-share.json`,
`share_hex`-containing files) outside an operator's keys dir or this
explicit list — it's a stray copy. Delete it and note it here.
