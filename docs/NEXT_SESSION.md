# Next Session — Plan (updated 2026-10-02, UTC+1)

## RESOLVED INCIDENT — node-3 state divergence (fix live-verified 2026-10-02)

- **2026-10-02 ~05:50 UTC:** `junoclaw-node-3` diverged at **h142868** on an
  **empty block** (`tx_count=0`). Network root `5765e54f…` vs node-3 local
  `8fccf8c4…`. It then logged `state_root mismatch — halting execution` on
  every subsequent block (11,495 consecutive) while the other 3 finalized
  fine — chain kept quorum at 3/4.
- **Forensics preserved:** `docker commit` → image `diverged-node3:h142868`
  holds the divergent DB for offline diffing.
- **Recovery verified:** wiped `devnet_node3_data`, re-created → state-sync
  adopted an anchor, caught up to tip (h154,960+) in seconds and resumed
  proposing.
- **Root cause CONFIRMED + FIXED (2026-10-02):** the divergence actually
  originated at **h142867** (a Wasm `Instantiate` tx), surfacing as the
  state-root mismatch at h142868. `cosmwasm_vm::Cache` stores compiled wasm
  modules + source blobs in a **node-local** dir (`{data}/wal/cache/modules`)
  that state-sync snapshots never carry. Node-3 (freshly state-synced) had
  the committed `CodeInfo` checksums but NO bytecode on disk → `get_instance`
  failed there while the 3 warm nodes executed → divergent state root.
- **Fix (mirrors wasmd — wasm blob is committed state, cache is a cache):**
  - `wasm::keeper`: new `CODE_BYTES: Map<&[u8], Vec<u8>>` under the `wasm/`
    namespace — `StoreCode` and genesis `init()` now persist the raw blob
    keyed by checksum (metered, app-hash-covered, snapshot-carried).
    `WasmQuery::CodeInfo{include_wasm}` serves the blob from KV, not the
    node-local dir. `Pin` hydrates before pinning.
  - `wasm::vm::cache`: new `VmCache::ensure_cached()` called at the top of
    every `get_instance` path (instantiate/execute/migrate/sudo/reply/query).
    It REQUIRES the committed blob (deterministic error everywhere if absent)
    and, on local miss, `store_code`s it back into the VM cache — verifying
    the re-derived checksum. Deliberately unmetered: cache warmth is
    node-local, so metering it would itself be a divergence.
  - Regression test `cold_cache_hydrates_from_committed_state` seeds KV +
    empty cache dir → instantiate succeeds and warms the cache.
  - `layer-app`: 50/50 tests green. StoreCode gas limit raised in tests
    (metered blob write is real state cost, as in wasmd).
- **Deployment caveat:** existing devnet state predates `CODE_BYTES` — code
  stored before this fix has checksums but no committed blob, so a cold
  node still fails on it. Options: fresh genesis (recommended for devnet),
  or a one-shot upgrade migration that reads `wal/cache/modules` on a
  canonical node and injects the blobs via a deterministic mechanism
  (e.g. gov StoreCode re-tx). Soak/monitor must restart on the fixed image.
- **LIVE VERIFICATION (2026-10-02, fresh genesis on fixed image
  `698ac45a`):** StoreCode `cw20_base.wasm` committed h2880
  (tx `30D3EFB9…`, code_id=2); node-3 data wiped → state-sync adopted
  h6332 (multi-peer quorum anchor, cold VM cache); two bad-JSON
  instantiates at h24759/h25241 errored IDENTICALLY on all 4 nodes
  (deterministic contract error — hydration already worked); successful
  instantiate h25639 (tx `7DA5616C…`, code=0, gas_used=4,878,992,
  contract `juno18hgxtqvzc8s7auxtcup00umr8sweaf3py87qx8d7vxfmx3w92ay0tyq0dks7w`).
  Node-3 has finalized ~32,600+ post-instantiate blocks with ZERO
  `state_root mismatch` halts — the fail-stop check (node.rs:560,
  `payload.state_root != local_root` → halt) proves node-3's covered
  state equals the certified root at EVERY height. **FIX VERIFIED.**
- **IMPORTANT WATCH-OUT — `app_hash` ≠ `state_root`:** the `app_hash=`
  field in finalize logs is `storage.app_hash()` = `FastHasher`, a
  rolling WRITE-HISTORY hash (seeded from `APP_HASH_KEY`, folds each
  commit's set/remove ops). A state-synced node's history is
  `[zeros → bulk import commit → new commits]` vs donors' incremental
  commits → its `app_hash` diverges PERMANENTLY from the first post-sync
  block (observed h6333) even with identical consensus state. Consensus
  binds `BlockPayload.state_root` (Merkle over non-`_` KV) — compare
  `digest=` (sha256 of certified payload, binds state_root) or watch
  for fail-stop halts, NOT `app_hash=`. `monitor-s4.ps1` updated
  accordingly (digest comparison + halt-line scan).

## Ops running (detached PS processes)

- `monitor-s4.ps1` (26h) — polls all 4 nodes' logs every 30s; alerts on
  certified-payload **digest** mismatch (binds `state_root` — do NOT use
  `app_hash`, it is the FastHasher write-history hash and diverges
  permanently on state-synced nodes), `state_root mismatch` fail-stop
  halt lines, lag>10, stall>90s → `monitor-s4.log`.
- `soak-c9.ps1` (24h) — chaos events every ~60–120min: kill+restart /
  90–180s network partition / compose recreate, rotating victim, recovery
  measured via tip delta → `soak-c9.log`. **Byzantine-proposer leg needs a
  patched image — no fault-injection flag exists yet; build one for full
  C9 credit.**
- **C9 FINDING + FIX (2026-10-02 ~20:10Z):** partition leg (EVENT#3,
  node-3, 91s) wedged the node silently for ~40min — NOT a node bug:
  `docker network connect` without `--ip` reassigned node-3 to
  `172.28.0.2` while compose + all `[[peers]]` pin it to `172.28.0.13`.
  Peers dialed a stale address; node showed idle CPU, zero consensus
  activity, and even post-restart its outbound never linked. Fix:
  `docker network connect --ip 172.28.0.$((victim+10))` in the partition
  leg (soak-c9.ps1 patched). Recovery: restart + IP re-pin → relays
  resumed instantly; gap backfill + finalize-missing-digest fetch then
  resolved the ~9.8k-block hole (missing digests arrive ~ms after
  `finalized payload unavailable` errors). Residual question: why
  node-3's OUTBOUND dials to peers (0.10–0.12, unchanged IPs) did not
  restore connectivity in ~25min post-restart before the IP re-pin —
  commonware lookup tracker may gate dialing on the registered listen
  addr; worth a unit-level look but not consensus-critical.
- **C9 FINDING #2 — payload-fetch stall on large gap (chain bug, real):**
  after the IP re-pin, node-3 fell ~10k blocks behind and could NOT
  recover via backfill: ~5min of scattered solicited inserts
  (h83–84k), then ZERO payload pushes received while consensus traffic
  (proposal verify/rejects at current views) kept flowing — so the
  consensus P2P channel stayed up but the payload-relay/fetch channel
  stalled. Solicited-mark expiry alone doesn't explain it: digest
  requests carry permanent marks (`requested` set, no TTL under 65k)
  yet `finalize`'s needed payload never arrived, i.e. replies stopped
  entirely — consistent with peers dropping node-3 from their relay
  sender set (connection flap during the wrong-IP window never
  re-linked, or per-peer rate limiting after the backfill request
  flood: backfill_tick re-sends ALL missing ranges every 250ms —
  8 range reqs/tick = ~32 req/s/peer, each triggering up to 64 pushes).
  **FIXED (commit pending, tested 67/67 slay3rd):** `node.rs` now skips
  heights with fresh in-flight marks (`HEIGHT_REASK_AFTER`=2s re-ask),
  keeps solicited marks valid for `SOLICITED_HEIGHT_TTL`=30s so delayed
  replies still land, and caps solicited inserts at
  `MAX_PENDING_PAYLOADS_TOTAL`=16384. Regression tests:
  `test_backfill_tick_does_not_reflood_inflight_heights`,
  `test_backfill_tick_reasks_after_reask_window`. DEPLOY GATE: do NOT
  rebuild the docker image until the soak ends — a --force-recreate
  event mid-soak would boot a mixed-version node. Recovered node-3 via full
  state-sync (wipe volume → certified snapshot → live at tip in
  seconds, back to peers=3) — ALSO proof that state-sync, not
  backfill, is the right recovery for big gaps; consider gating:
  gap > N heights → offer state-sync path. Debug-level visibility of
  dropped pushes (`Err(reason)` arm in the relay) is needed to pin the
  exact stall mechanism — enable trace logging in a future repro.

## Landed this session (2026-10-01) — LIVE-VERIFIED on devnet

- **PQ protocol auth Phase 2a — hybrid consensus LIVE-VERIFIED.** All 4
  devnet validators finalizing `0x01`-tagged hybrid certs (BLS threshold
  cert + signer bitmap + MAYO2 quorum, `cert_len=616`), heights into the
  thousands. Docker keygen emitted MAYO2 identity for every validator;
  `hybrid_consensus = true` booted clean after wipe. Coded per
  `docs/PQ_PROTOCOL_AUTH.md` §3–§5: `HybridScheme` wraps the BLS12-381
  threshold scheme (`HybridSignature` = BLS partial + 186-byte MAYO2 sig
  over identical namespaced bytes); `HybridCertificate` requires quorum on
  both halves.
- **Two real bugs found by the live run — both fixed:**
  - *Randomized-signature equivocation:* MAYO draws a fresh salt per sign,
    so journal replay / retransmit of an identical vote produced different
    bytes. Simplex compares votes byte-for-byte → read as *conflicting*
    vote → sender blocked → consensus stalled at view 1. Fix: `sign`
    memoizes MAYO sigs per signed message (`sig_cache`,
    `app/slay3rd/src/hybrid_scheme.rs`).
  - *Unanchorable genesis snapshot:* donors lazily cached a height-0
    export; joiner demanded a certified tip at h0, which never exists →
    fatal `anchor quorum failed`. Fixed both sides: `cached_snapshot`
    refuses h0 exports (`grpc.rs`); `adopt_snapshot` skips h0 offers and
    takes the highest offered height (`state_sync.rs`).
- **Multi-peer state-sync anchor LIVE-VERIFIED.** Node-3 adopted a
  snapshot at h7,085 via `min_anchor_agree = 2` quorum on certified
  payloads (peers node-0/node-1), joined consensus, caught up to tip in
  seconds.
- **Stubs closed:** `timeout_height` enforced in ante (check_tx +
  deliver, `keeper.rs`, unit-tested); M1 leader-forwarding tx propagation
  (non-leader gossips to next round-robin leaders over authenticated P2P);
  MAYO-1/2/3/5 NIST KATs wired into `packages/junoclaw-mayo-verify/
  tests/kat.rs` (fixture integrity + corruption negatives); MAYO identity
  in Phase A ceremony (`ShareRequest`/`SharedValidator` carry
  `mayo_public_hex`, attestation binds `ed25519_pk || mayo_pk`,
  all-or-none enforced).
- **Crash vector closed** (carried): `Account::Smart` `todo!()` →
  deterministic `TxError::SmartAccountSigner`.
- `cargo test -p slay3rd -p layer-app` — 65 + 49 passed, 0 failed.

## Ordered next steps (deterministic)

Each item: what to do + the check that proves it done.

### 1. Hybrid-consensus negative tests — ✅ ALL PASSED live (2026-10-01)
- Quorum loss: `docker stop` node-2+node-3 → chain stalled ~10 min
  (last finalize h46555 @14:31:08Z, then silence on both survivors —
  neither BLS nor MAYO quorum reachable at 2-of-4). `docker start` →
  all 4 resumed finalizing @14:42Z with no manual intervention.
- Corrupted MAYO table: flipped one byte of `mayo_public_hex` in
  validator-0/keys.json → node-0 refused at startup:
  `ERROR mayo_public_hex does not match validator_mayo_public_keys at
  our sorted position` (main.rs:1204), clean exit. Restored → resumed.
- Byzantine state-sync donor: spawned rogue 1-validator chain
  (`devnet/config/rogue.toml` + `devnet/rogue-keys/`, threshold 1-of-1,
  chain_id=junoclaw-rogue) on the devnet network at 172.28.0.20.
  Joiner (`devnet/config/joiner-byz.toml`, peers=[rogue, honest],
  min_anchor_agree=2) took the rogue's snapshot, then hit:
  `anchor disagreement: peer 172.28.0.10:9090 served different
  certified payload bytes than 172.28.0.20:9090 — refusing to pick a
  winner` → "state-sync failed — refusing genesis fallback", exited 0,
  never adopted. Rogue+joiner containers removed after the test.

### 2. G0 close-out (TESTNET_LAUNCH_PLAN §2–§8)
- **C2 timestamps** — view-derived OK for G0; pre-G1 switch to proposer
  wall-clock bounded `parent < t ≤ local_now + 2s`.
- **C7 bench** — ✅ done + live-verified. `node.rs` logs `exec_ms` per
  finalized block; `tx-sender bench --kind {bank|bud1..5} --count N`
  sends paced txs (fresh bech32 bud children per run, MAYO pk lens
  1420/4912/2986/5554 B). Gate: p99 < 50% of `leader_timeout_ms` (1500 ms).
  Measured on devnet (n=100/leg): bank p50=701 p99=1195 ms; bud2 p50=728
  p99=1048 ms; bud5 p50=734 p99=953 ms — all PASS with ≥20% headroom.
  Note: `code_id=1` is the genesis root contract; stored wasm lands on
  `code_id≥2` — resolve via `code-info`, don't assume (store-code print
  now says this).
- **C9 soak** — ≥4 validators, ≥24h, leader kill + partition + restart +
  byzantine proposer.
- **M4 sender ordering** — ✅ done. `TxMeta{sender,sequence,store_code}`
  plumbed at both admission paths (BroadcastTx + gossip) before `check_tx`
  consumes the tx; `peek_batch` k-way merges per-sender queues sorted by
  sequence (FIFO fairness across senders via queue-head arrival). Live
  note: `check_tx` rejects future sequences, so seq-gaps can't sit pending
  — ordering protects the in-window race. 15 mempool tests pass.
- **M7 limits** — ✅ done. `DEFAULT_MAX_PER_SENDER = 64` (`SenderFull`
  error → `resource_exhausted`); `MsgStoreCode` txs get the 8 MiB
  `MAX_STORE_CODE_TX_BYTES` cap instead of 2 MiB `max_tx_bytes`.
- **Q2** — ✅ done + live-verified. `App::set_tx_response`/`get_tx_response`
  persist prost-encoded `TxResponse` under `_txres/<hash>`
  (app_hash-excluded); `execute_block` writes through alongside the
  in-memory 100k cache, and `get_tx` falls back to disk on a cache miss
  then warms it. Live: tx 9770648F… committed h108394, `docker restart
  junoclaw-node-0` (in-memory index wiped), `get-tx` still resolved the
  result + decoded body from disk. Pruning/capping is a follow-up knob.
- **Q3/Q7** — ✅ done + live-verified. `get_tx` recovers the committed
  raw bytes from the `BlockPayload` at `TxResponse.height` and returns the
  decoded `Tx{body,auth_info,signatures}`; `tx-sender get-tx --wait <s>`
  polls every 500 ms until committed and prints msg type URLs. Live:
  send tx 2EF194BA… at h102025, `get-tx --wait 15` returned
  `messages: 1 / msg: /cosmos.bank.v1beta1.MsgSend`.
- **S4 monitoring** — `app_hash` agreement alerts across validators.

### 3. PQ-1 fuzz gate (G1 blocker)
- ✅ exists: `junoclaw-mayo-verify/tests/fuzz_inputs.rs` (seeded-xorshift
  malformed pk/sig/msg sweep over MAYO-1/2/3/5, `--ignored` soak variant),
  `packages/std/tests/fuzz_pubkey.rs` (pk-hash path).
- Still needed: Bud contract arithmetic fuzz — the deployed `Bud`
  member-tree contract's source is NOT in `junoclaw-chain` or
  `junoclaw/contracts` (searched); locate before fuzzing. ML-DSA-44/65/87
  KATs — no ML-DSA verifier exists in this workspace yet (post-G2 per
  plan, but KAT leg belongs to PQ-1).

### 4. G1 ceremony + ops
- Ceremony rehearsal with external validators (`ceremony-test/` exists;
  MAYO step is now plumbed, needs a real-entropy run).
- Faucet, status page, relayer supervisor, runbook.

## Watch-outs (carry-forward)
- `packages/junoclaw-mayo-verify` is VENDORED — mirror edits to
  `junoclaw/crates/junoclaw-mayo-verify`.
- `PubKey`/`Account` enums are serde-JSON in state: append-only.
- Mixed-version P2P: tag 2 (HeightRange) and hybrid certs are only
  understood by new images — rebuild all validators at once.
- Hybrid certificates are larger: 4-sig hybrid cert ≈ classical + ~750 B
  (`cert_len=616` measured). `mailbox_size`/`replay_buffer` unchanged;
  watch `cert` channel metrics.
- MAYO sig memoization is per-process; bounded at 8192 entries. If a
  replay storm ever exceeds it the cache just signs fresh — no
  correctness impact, only a possible equivocation-style duplicate under
  retransmit, which is now the documented behavior.

---

# Historical — 2026-09-29 session

## Verified LIVE this session (all on devnet)

- **Backfill live-verified.** Rebuilt `junoclaw-chain:latest` (`ae230240214e`),
  `docker compose up -d` all 4 validators, stopped `junoclaw-node-2` at
  ~h164,800 (14:45:18Z), restarted 14:54:26Z with ~700-block gap. Node fetched
  missing payloads via `HeightRange` backfill, executed ~1,600 heights in
  ~15 min (~1.7× live rate), rejoined in full lockstep (identical `app_hash`,
  proposing + certing). No divergence, no halt.
- **Dockerfile dep-cache bug FIXED.** `docker/Dockerfile.slay3rd` fingerprint
  purge only cleared `layer-*`/`slay3rd-*`; `junoclaw-mayo-verify` fingerprint
  survived → stale stub rlib → `E0432` in `layer-std`. Purge now covers
  `junoclaw-*`, `tx-sender-*`, `bls-relayer-*` + stale rlib files. (Same bug
  class hits any future non-`layer-*` workspace crate — keep the globs
  current when adding members.)
- **Hybrid PQ tx live-verified END-TO-END.**
  - `tools/hybrid-sign` (standalone, workspace-excluded): `keygen` + `sign`,
    sriracha-mayo + k256, emits TxRaw hex.
  - `docker/Dockerfile.hybrid-sign` → `junoclaw-hybrid-sign:latest`
    (rust:bookworm + cmake; sriracha can't build on this Windows host).
  - `tx-sender broadcast --tx-file <hex>` subcommand added.
  - Flow proven: deployer funded `juno16ct0r…` (5M ujclaw) → hybrid-sign
    MsgSend (seq 0, account_number 17 = `FIXED_ACCOUNT_NUMBER`) → broadcast →
    committed @ **h165266**, `code=0`, gas_used=109,423, tx
    `64A0E72C2877323D070631412EE0AEA4DC8A4BC556133D19E5308DF1D7ABE4D6`.
    Consensus ran `junoclaw-mayo-verify` + secp256k1 — both required.
  - Devnet keys still funded: secp_sk `1abf42…`, mayo_sk `f8b501…` at
    `juno16ct0r0y6jwdddx5qtjgku6e5gg962q6ays0ee7` (seq now 1).
- **Phase 2 design doc**: `docs/PQ_PROTOCOL_AUTH.md` — hybrid validator
  identity (ed25519+MAYO) + `HybridCertificate` (BLS-threshold AND k-of-n
  MAYO bitmap cert), grounded in commonware simplex `bls12381_threshold`
  wiring in `main.rs`.
- **Community article**: `junoclaw/articles/HYBRID_TX_AND_BACKFILL_LIVE_2026_09_29.md`.

## (Historical) What landed earlier today

## What landed this session

- **Bulk backfill (largest testnet blocker — code landed, live verify pending)**
  - `FetchRequest::{Digest, HeightRange}` in `app/slay3rd/src/node.rs`
  - `PayloadStore` height→digest reverse index + `get_by_height`/`contains_height`
  - `DEFAULT_RETAIN_HEIGHTS` 1,024 → 65,536 (max servable backfill depth)
  - `LayerNode::backfill_tick` (driven every 250 ms from `main.rs` Task D):
    requests all missing heights tip+1..max_seen in ≤64-height ranges
  - `max_seen_height` atomic: bumped on EVERY peer push incl. rejected ones —
    that's how a lagging node learns the peer tip
  - Height-solicited replies bypass `PEER_PAYLOAD_LOOKAHEAD`/pending cap
    (same rule as digest-solicited)
  - P2P wire: new tag `PAYLOAD_MSG_REQUEST_RANGE=2` (u64be start + u16be count),
    peer serves one PUSH per held height, capped at `BACKFILL_BATCH_SIZE`
  - Tests: `test_height_solicited_payload_bypasses_lookahead`,
    `test_backfill_tick_requests_missing_range`,
    `test_payload_by_height_serves_pending`, `get_by_height_roundtrip`
  - `cargo test -p slay3rd --lib`: 53/53 green

- **PQ protocol auth Phase 1 — hybrid accounts (chain-side complete)**
  - `PubKey::HybridSecp256k1Mayo { secp256k1, mayo_variant, mayo_pk }` in
    `layer-std`; `validate_signature` requires BOTH sigs over `message_hash`
  - `MayoVariant` (Mayo1/2/3/5) dispatch to vendored `junoclaw-mayo-verify`
    at `packages/junoclaw-mayo-verify` (keep in sync with
    `junoclaw/crates/junoclaw-mayo-verify` — comment in its Cargo.toml)
  - Wire: `Any{type_url="/junoclaw.crypto.HybridSecp256k1Mayo",
    value=[variant:1][secp_len:1][secp33][mayo_pk]}`;
    signature = `[64B secp][mayo_sig]`; helpers
    `to_hybrid_any_bytes`/`from_hybrid_any_bytes`/`pack_hybrid_signature`
  - Address: `sha256("junoclaw-hybrid-v1" || wire)` → ripemd160 —
    domain-separated from secp accounts
  - Parse/encode in `layer-cosmos/pubkey.rs` (via `SignerPublicKey::Any`)
  - Gas: `GAS_COST_PQ_SIG_VALIDATION = 25_000` extra in auth keeper
  - Tests green: dual-sig verify vs sriracha-generated vector
    (`packages/cosmos/src/mayo_test_vector.rs`), Any round-trip,
    domain separation
  - **Vector regeneration**: `C:\cosmos-node\tmp\gen-mayo-vector` — run the
    prebuilt binary in Docker:
    `docker run --rm -v C:\cosmos-node\tmp\gen-mayo-vector:/w -w /w rust:bookworm bash -c "./target/release/gen-mayo-vector > /w/vector.txt"`
    (sriracha needs cmake — NOT buildable on this Windows host)

- **Docs updated**: `ROAD_TO_TESTNET.md` (backfill + hybrid auth status),
  `KEY_MANAGEMENT.md` §6 cold backup (paper/metal + encrypted-at-rest),
  PQ article scorecard + checklist.

## Ordered next steps

### 1. Negative test — MAYO-corruption variant DONE (2026-09-29)
- Signed a second hybrid tx (seq 1) with `hybrid-sign`, flipped one byte in the
  MAYO sig tail of `signatures[0]`, broadcast via `tx-sender broadcast`:
  → `check_tx failed: Tx(InvalidSignature)` — REJECTED as designed.
- The identical valid tx committed @ h169044 (`code=0`, gas_used=116,144) —
  rejection is signature-specific, not body/sequence.
- Still open variants: truncated sig (secp-only), wrong MAYO variant pubkey —
  same mechanics, low priority since the verifier path is proven live.
- NOTE: `tx-sender.exe` on disk must be rebuilt for `broadcast` —
  `cargo build --release -p tx-sender` (the stale binary predates the
  subcommand).

### 2. State-sync — DESIGN WRITTEN (`docs/STATE_SYNC.md`, 2026-09-29)
- Flat sorted-leaf Merkle means NO per-chunk proofs needed: joiner downloads
  full KV dump in checksum'd chunks, recomputes `state_root_over`, compares
  to `root_H` from a BLS-certified `BlockPayload_H`. One root check
  authenticates every byte.
- ~~Implement~~ — DONE 2026-09-29 (flag-gated, config `state_sync.peer_grpc`):
  - Producer: `App::snapshot_export` — single-pass chunked dump of all
    consensus KV (`_` sidecars excluded), `[klen|key|vlen|val]` records,
    ≤8MiB chunks, per-chunk sha256, embedded `state_root` recomputed on
    the SAME storage reader as the records.
  - Wire: `proto/layer/statesync/v1/query.proto` + checked-in
    `layer.statesync.v1.rs` (mirrors lightclient codegen — no buf/protoc
    needed for the Rust side), `StateSyncQuery` impl on `LayerGrpcService`
    with a cached export refreshed every 1000 blocks, server registered
    in `main.rs` with 12MiB encoding cap.
  - Joiner: `App::snapshot_import` — verify-before-write whole-dump root
    check, atomic commit + LAST_BLOCK + `load_from_storage` repopulates
    InnerData from the dump's own `app/state`; `slay3rd/state_sync.rs`
    `adopt_snapshot` fetches chunks + certified `BlockPayload` at H+1
    (only trust anchor), cross-checks advertised root, adopts;
    `NoStoredState` + `state_sync.peer_grpc` → adopt, failure is FATAL
    (never silently falls back to genesis — would fork state).
  - Tests green: export round-trip + tamper/drop/shuffle/truncation,
    import adopt/refuse + identical re-export, gRPC list/chunk/error
    paths. `cargo test -p layer-app -p slay3rd`: 101 pass.
- REMAINING: live devnet verify — spin a fresh node-4 with
  `state_sync.peer_grpc` pointed at node-0, confirm adopt + resume.
- Anchor switches to `HybridCertificate` cleanly in Phase 2.

### 3. Phase 2 impl (from `docs/PQ_PROTOCOL_AUTH.md`)
- 2a: `mayo_{private,public}_hex` in key material + dual `hybrid_sig`
  (ed25519+MAYO) on consensus messages behind `--hybrid-consensus` flag.
- 2b: `HybridCertificate` envelope (tag 0x01) + MAYO bitmap cert assembly.
- Open question: on-chain MAYO verify for light clients (WASM gas
  benchmark vs relayer attestation) — doc §6.

### 4. Smaller items
- ~~`layer-cosmos` fixture failures~~ — DONE 2026-09-29: stale bech32
  fixtures from the layer→juno prefix swap (checksums never recomputed).
  Re-encoded payloads via bech32::decode→AccountId regen; `parse_simulate`
  hex fixture rebuilt with a real signed juno-prefixed MsgSend. 11/11 pass.
- ~~`Simulate` gRPC impl~~ — DONE 2026-09-29: `App::simulate` runs the full
  `execute_tx` path (auth + all msgs, metered) on a `ScratchTx` overlay over
  a storage reader — nothing persists, no mempool interaction. Handler maps
  GasInfo + events into `SimulateResponse`; errors surface in `result.log`.
  Deprecated `tx` field rejected (tx_bytes only); `msg_responses` left empty
  (layer MsgData is a Rust enum, not proto Any). `test_simulate` passes.
- Fuzz `junoclaw-mayo-verify` + pk-hash path.

## Watch-outs
- `packages/junoclaw-mayo-verify` is a VENDORED copy — edits must be
  mirrored to `junoclaw/crates/junoclaw-mayo-verify` or contract/chain
  verification drifts apart.
- `PubKey` gained a variant — `Account::External{pubkey}` is serde-JSON in
  state; append-only variant order keeps old state readable (don't reorder).
- Old nodes won't understand `PAYLOAD_MSG_REQUEST_RANGE` (tag 2) — mixed
  versions are fine (unknown tag → warn+skip) but lagging nodes on old
  images won't serve or request ranges. Rebuild all four at once.
