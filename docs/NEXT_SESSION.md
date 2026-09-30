# Next Session — Plan (updated 2026-09-29 ~16:15 UTC+1)

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
