# State-Sync Design — Bootstrapping Without Replay

*Status: design — 2026-09-29. Builds on: live-verified bulk backfill
(`FetchRequest::HeightRange`), `App::compute_state_root` / `state_proof`
(`packages/app/src/app.rs`), BLS finality certs (`BLS_LIGHT_CLIENT_SPEC.md`).*

## 1. Problem

Backfill parallelises **payload acquisition** but not **state construction**:
a node at height H must still *execute* blocks 1..H to arrive at app state.
Measured live: ~1.7× production rate → catching up 200k heights ≈ 3.5 days.
Fine for a validator that restarted after lunch; disqualifying for a new
validator, a restored backup, or a full node joining mid-flight.

State-sync inverts the cost: **trust one finalized `state_root`, download the
state underneath it, verify it, resume consensus from the next height.**

## 2. The anchor already exists — nothing new to trust

Every `BlockPayload` carries `state_root` (`block.rs:38`) — the Merkle root
over all non-`'_'`-prefixed KV entries, committed atomically at
`finalize_block`. Every finalized block carries a BLS12-381 threshold
certificate. So `state_root @ H` is already consensus-authenticated: it's the
same trust anchor the BLS light client verifies today.

A joining node picks a certified height **H** (any height with a retrievable
finality record — bounded by the donor's pruning tier: 540,000 heights on
validators, unbounded on archive nodes), pins:

- `cert_H` — BLS threshold cert over `BlockPayload_H`
- `root_H` — `BlockPayload_H.state_root`, extracted after cert verification

No sidecars, no trusted flags. Everything downstream is verified against
`root_H`.

## 3. Why our flat Merkle tree makes this *simpler* than Cosmos'

`state_root_over` (`app.rs:940`) builds a domain-separated binary Merkle tree
over the **sorted leaf list** of all consensus KV:

```
leaf = sha256(0x00 || key || value)
node = sha256(0x01 || left || right)    (odd nodes promote)
```

Cosmos state-sync needs per-chunk IAVL proofs because chunks are partial-tree
artifacts. Ours doesn't: the leaf set IS the state. A joiner that receives the
complete sorted KV dump can recompute the root locally — and one root check
authenticates **every byte** of the download. No per-chunk proofs needed.

Consequences:

- **Chunking is a transport detail, not a proof detail.** Split the dump into
  fixed-size chunks for resumable/parallel transfer; each chunk carries a
  plain sha256 checksum for corruption detection. The cryptographic
  authentication happens once, at the end, via `root_H` recomputation.
- **Tampering is detected, always.** Wrong key, wrong value, extra key,
  dropped key, reordering — all change the recomputed root. Any mismatch →
  discard, refetch, or fail loudly. (A malicious peer cannot craft a fake
  dump that recomputes to `root_H` — that's second-preimage resistance.)
- **Serving cost is O(state)** — the same full iteration `state_root_over`
  already does. No auxiliary structures to maintain.

## 4. Protocol

New gRPC pair on the existing service (mirrors Cosmos' snapshot API):

```proto
message ListSnapshotsRequest {}
message ListSnapshotsResponse {
  repeated SnapshotMeta snapshots = 1;  // {height, state_root, format, chunks, total_bytes}
}
message LoadSnapshotChunkRequest {
  uint64 height = 1;
  uint32 format = 2;
  uint32 chunk  = 3;
}
message LoadSnapshotChunkResponse { bytes chunk = 1; bytes32 checksum = 2; }
```

**Producer side** (`packages/app`):
1. `snapshot_export(H)` — one storage `reader()` at LAST_BLOCK ≥ H:
   iterate `range(Ascending)` skipping `_`-prefixed keys (exactly the
   `state_root_over` filter), emit length-prefixed `key||value` records into
   ≤8 MiB chunks, each with a sha256 chunk checksum.
2. Snapshot metadata cached so repeat requests don't re-iterate.
3. Offer only heights with a finality record — a snapshot you can't anchor
   to a cert is worthless to a joiner.

**Joiner side** (slay3rd, `[state_sync]` config — `peers`,
`min_anchor_agree`):
1. `ListSnapshots` across configured peers; the first peer that serves a
   complete dump is the donor (advertised metadata untrusted).
2. **Multi-peer anchor**: query `Block(H)` and `Block(H+1)` on the
   lightclient service of EVERY configured peer. All responding peers
   must serve byte-identical `payload_bytes` — it binds height,
   timestamp, parent_digest and state_root. `min_anchor_agree` (default
   2) respondents minimum; any disagreement is Byzantine evidence →
   abort. The joiner never verifies the cert itself — the validator set
   lives inside the state being imported; quorum agreement is what
   prevents one malicious peer from pinning a fabricated root.
3. Download chunks (any order, retry-tolerant; checksum per chunk).
4. Import into a *staging* store; iterate staged KVs in ascending order,
   recompute `state_root_over`; **require equality with `root_H`**.
   Mismatch → abort, never partially adopt.
5. On match: promote staged store to live, set `LAST_BLOCK = H`,
   `_state_root = root_H`, then let the existing machinery take over —
   `backfill_tick` fetches payloads H+1..tip, finalize-driven execution
   resumes. Sidecar records (certs, timestamps) get populated by normal
   operation.

## 5. What this deliberately does NOT do

- **No per-chunk Merkle proofs** — the whole-dump root check subsumes them.
- **No incremental/delta sync** — needs a versioned store (JMT/IAVL-style);
  that's the documented upgrade path in `app.rs:854`. Flat recompute is fine
  at devnet/testnet state sizes.
- **No trusted attestations** — the anchor is the BLS-certified payload
  the chain already produces, corroborated by `min_anchor_agree`
  independent peers (single-peer `peer_grpc` is gone: one peer could
  have fabricated payload + snapshot consistently). The cert itself is
  not re-verified at join time — the validator set it authenticates
  against lives inside the imported state. Weak-subjectivity caveat:
  the anchor height should be recent enough that the peer set is still
  majority-honest for your purposes (standard for all checkpoint-based
  sync).
- **`_`-sidecar keys are not synced** — they're per-node operational data
  (payloads, cert records), not consensus state; the node regenerates them.

## 6. Failure modes

| Failure | Behaviour |
|---|---|
| Tampered chunk | sha256 checksum fails → refetch chunk |
| Censored/extra keys | recomputed root ≠ `root_H` → abort import |
| One peer lies about anchor | payload bytes differ from quorum → abort |
| Donor peer unreachable | try next configured peer for chunks |
| < min_anchor_agree peers reachable | abort (no silent degrade) |
| Stale snapshot (> retain window) | ListSnapshots won't offer it |
| Root matches but wrong height | impossible — `root_H` binds to H's payload via cert |
| Import interrupted mid-way | staging discarded; restart is idempotent |

## 7. Effort estimate

- `snapshot_export` + chunk streaming in `packages/app`: ~1 day (reuses the
  `state_root_over` iteration verbatim).
- gRPC surface: half day — same shape as the existing `proof` handler
  (`grpc.rs:345`).
- Joiner staging/import/root-check: ~1–2 days; the storage abstraction
  (`layer_storage::MemoryStore::import` exists for tests, `app.rs:932`)
  already gives us a staging substrate.
- Total: **~3 days to a working flag-gated implementation**, dominated by
  the joiner's staging-store plumbing.

## 8. Operator recovery procedure (production-reviewed 2026-10-27)

The `NoStoredState` entry path can be reached two ways; they are NOT
equivalent:

- **`force = true` under `[state_sync]`** — startup runs
  `remove_dir_all({data}/app)` BEFORE `adopt_snapshot` validates
  anything. Blast radius is app state only (`{data}/consensus`
  journal and `keys.json` are untouched), but the flag is a loaded
  footgun: left set, every restart — including a crash AFTER a
  successful sync under `Restart=on-failure` — re-wipes app state.
  Known one-shot hazard; do not use it when a backup matters.
- **Move-aside (reviewed, preferred)** — reaches the identical
  `NoStoredState` path with zero deletes:

  ```
  systemctl stop junoclaw        # manual stop: on-failure restart does NOT fire
  mv {data}/app {data}/app.bak-YYYYMMDD   # the move IS the backup
  # {data}/consensus (vote journal — anti-double-sign history) and
  # keys.json stay in place. node.toml: [state_sync] present, NO force.
  systemctl start junoclaw
  ```

  If the sync attempt fails (`exit(2)` → supervisor restart): a
  partially-imported `app/` can wedge `load_from_storage` — stop,
  `rm -rf {data}/app` (partial only; the `.bak` is intact), start,
  retry. Full rollback: stop, `rm -rf {data}/app`,
  `mv {data}/app.bak-YYYYMMDD {data}/app`, start — restores the
  exact prior state. Delete `.bak` only after the seat's bit is
  back in certificates.

Validation ordering (why the window before `snapshot_import` is
safe): `adopt_snapshot` performs ALL authentication first —
snapshot fetch, H+1 certified-payload quorum + `state_root` match,
H tip-payload agreement — and writes nothing until
`snapshot_import`. A validation failure exits before any app
mutation; only the import itself can leave partial state.

**Margin gate, reframed** (from the idx2 recovery review): "N-1
signers in certs before stopping" is unsatisfiable when the
recovering seat IS one of the absent bits — its resync can't be
preconditioned on its own return. The satisfiable gate: all other
seats process-alive + mesh-connected (cert bitmaps over recent
views + donor liveness), no concurrent maintenance fleet-wide, and
chain at/above quorum. A resync attempt cannot reduce margin below
the state where the seat is already absent.

## 9. Interaction with Phase 2 (`PQ_PROTOCOL_AUTH.md`)

When hybrid certificates land, the anchor becomes `HybridCertificate`
(BLS-threshold AND k-of-n MAYO). Nothing else in this design changes —
`root_H` is extracted from the payload the same way; only the cert-type
check differs. Design once, survive both regimes.
