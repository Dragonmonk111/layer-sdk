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
finality record — bounded by `DEFAULT_RETAIN_HEIGHTS` = 65,536 today), pins:

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

**Joiner side** (slay3rd, `--state-sync` flag or `statesync { enabled,
trust_height }` config):
1. Backfill normally until a certified payload ≥ `trust_height` is seen;
   extract `root_H` from `BlockPayload_H` (cert already verified by
   consensus layer).
2. `ListSnapshots` → pick snapshot at exactly H (or nearest ≤ H).
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
- **No trusted attestations** — the anchor is the BLS cert the chain already
  produces. Weak-subjectivity caveat: trust_height should be recent enough
  that ≥2/3 of the signing set still overlaps (standard for all
  checkpoint-based sync).
- **`_`-sidecar keys are not synced** — they're per-node operational data
  (payloads, cert records), not consensus state; the node regenerates them.

## 6. Failure modes

| Failure | Behaviour |
|---|---|
| Tampered chunk | sha256 checksum fails → refetch chunk |
| Censored/extra keys | recomputed root ≠ `root_H` → abort import |
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

## 8. Interaction with Phase 2 (`PQ_PROTOCOL_AUTH.md`)

When hybrid certificates land, the anchor becomes `HybridCertificate`
(BLS-threshold AND k-of-n MAYO). Nothing else in this design changes —
`root_H` is extracted from the payload the same way; only the cert-type
check differs. Design once, survive both regimes.
