# Issue Triage Report

Generated: 2026-03-20
Repository: https://github.com/Lay3rLabs/layer-sdk/issues
Open issues reviewed: 25

---

## RECOMMEND CLOSING

These issues are obsolete, completed, superseded by project evolution, or otherwise no longer worth tracking.

---

### #144 — Upgrade to CosmWasm 2.0
**Verdict: CLOSE — already done**
The workspace is on `cosmwasm-std = "2.3.2"` with `features = ["cosmwasm_2_0"]`. This was completed.

---

### #12 — Staking: Add basic staking module
### #13 — Staking: Compatibility
**Verdict: CLOSE — superseded by project architecture**
These are 3-year-old issues reflecting an early vision of Layer-SDK as a traditional PoS chain. The project has pivoted to a WAVS/commitments-based architecture where "staking" is handled at the contract layer, not a native module. No progress has been made in 3 years. Close as won't fix / not planned.

---

### #11 — Wasm: Add custom hooks (twasm)
**Verdict: CLOSE — concept abandoned**
No body, no comments, no code, 3 years old. The twasm hook concept from the early days was never pursued. The current architecture doesn't have a place for this. Close as won't fix.

---

### #49 — Add simple CLI tool
**Verdict: CLOSE — fulfilled by external tools**
Jake and Ethan both acknowledged that `climb` CLI and `avs-tools-cli` serve this purpose. The remaining gap (proper README links to these tools) should be a new focused doc issue if still needed. Close this one.

---

### #69 — Plan proper app configuration
**Verdict: CLOSE — tracking issue no longer useful**
The issue body itself marks HomeDir and Jaeger as `DONE`. The remaining items (DB perf tuning, ABCI config, query gas max) have been open for 3 years without any traction. The `TODO: add more items here` comment makes this a zombie tracking issue. Close it and file specific issues when concrete gaps are identified.

---

### #163 — Rewrite faucet in Rust (climb)
**Verdict: CLOSE — primary work completed in climb repo**
The issue body notes this was redirected to `climb/issues/31` and `climb/issues/34`. Jake says it's done. The docker-compose integration piece, if still incomplete, should be tracked as a new narrowly-scoped issue.

---

### #76 — Simplify Args with Ctx/CtxMut
**Verdict: CLOSE (or downgrade to backlog)**
A 3-year-old code quality suggestion. There's no `Ctx`/`CtxMut` in the codebase today. With the Commonware branch reshaping the architecture significantly, the specific function signatures noted in this issue may no longer be the main offenders. If this refactor is still desired, re-open against the current codebase.

---

## KEEP OPEN

These issues remain valid, actionable, and worth fixing.

---

### Priority: High (bugs / crashes / correctness)

#### #177 — cometbft crash when trying to execute DAO proposal containing instantiate2
**Why still valid:** The chain crashes (EOF / panic) rather than returning a graceful error when a Stargate message it doesn't support is used during simulation. A crash is never acceptable — at minimum it should return a descriptive error. The root cause (unsupported message type in the executor) is still present.

#### #178 — Possible concurrency bug (needs more info)
**Why still valid:** Reproducible under load using `ghz` against both gRPC Cosmos queries and internal bank queries. ~0.1% error rate under sustained concurrent requests. Root cause not identified. Likely a race in the shared `Arc<Mutex<App>>` or in the underlying gRPC connection handling. Related to #162.

#### #162 — Possible bug with multiple transactions in a block
**Why still valid:** Semi-reproducible freeze when multiple transactions land in the same block from concurrent senders. May be related to #178. Since the Commonware branch is reworking consensus, this should be verified against the new implementation — but still needs to be tracked until confirmed fixed.

#### #71 — Add wasm query stack limit
**Why still valid:** No query depth limit exists. A contract can recursively call queries until a stack overflow crash. The fix is simple: cap query depth at 10. This is a safety/stability issue.

---

### Priority: Medium (missing features / important gaps)

#### #117 — Implement `/cosmos.staking.v1beta1.Query/Validators`
**Why still valid:** Required for frontend wallet and explorer compatibility. Many Cosmos apps assume this endpoint exists. Not implemented.

#### #109 — Ensure grpc-gateway decodes JSON in wasm messages responses
**Why still valid:** Wasm message fields come back as base64 in gRPC-gateway responses, breaking block explorers and frontends that need to display decoded messages. Ethan also noted there are unimplemented CosmWasm gRPC methods that would be lower-hanging fruit to address first.

#### #150 — State-sync getChanges should error if last sequence unavailable
**Why still valid:** Currently the state sync streams a subset of valid changes when WAL files have been cleaned up, silently giving clients an incomplete view. It should error and force a full resync. Correctness issue.

#### #155 — Add GitHub action to test tools
**Why still valid:** Jake claimed done but Ethan specifically pointed out that the `basic.yml` workflow excludes tools from the test run via `Cargo.toml` exclude lines. The tools themselves are not being CI-tested. The dispute was never resolved — check the current `basic.yml` to confirm.

#### #148 — Feature-gate `transport` in protobuf definitions
**Why still valid:** The upstream tonic issue (#1941) remains open. The proto crate includes tonic transport even when not needed, bloating wasm builds and causing issues in no-std/wasm targets. The ibc-proto-rs workaround is a reasonable interim solution.

#### #128 — Use Ethereum Style Addresses
**Why still valid:** No 0x address support exists yet. This is an architectural decision with real implications for wallet compatibility (Keplr/MetaMask), contract storage patterns, and the bech32 migration path. Actively discussed but not implemented.

#### #33 — Allow configuring minimum fee
**Why still valid:** No minimum fee configuration exists. This matters for testnet/mainnet deployments where zero-fee transactions should be rejected. Short body but the need is real.

#### #48 — Cache signature verification results
**Why still valid:** `secp256k1` verification is called on every `check_tx` and again on `finalize_block`. An LRU cache keyed on `(message_hash, pubkey, signature)` would give a meaningful throughput boost under load, particularly relevant given the concurrency issues in #178.

---

### Priority: Low (improvements / documentation)

#### #164 — README.md docker-compose.yml link
**Why still valid:** The README references `./docker-compose.yml` at the repo root, but that file doesn't exist there — it's at `localnode/docker-compose.yml`. Trivial fix, but it breaks the first-run experience for new contributors.

#### #125 — Document / design event syncing strategies
**Why still valid:** There's no documentation on how to subscribe to on-chain events, what events the chain emits, or what off-chain tooling is recommended. Important for anyone building on top of Layer-SDK.

#### #124 — Add filter to state sync
**Why still valid:** The state sync streams everything. A prefix filter would allow lightweight clients to subscribe only to relevant state (e.g., one contract's storage, or just bank balances). Described as a future improvement — still unimplemented.

#### #77 — Refactor wasm keeper
**Why still valid:** `keeper.rs` is still a large, mostly monolithic file. Ethan's original request to split it into multiple files and rename `VmCache` to something less confusing remains unaddressed. Low priority but worth keeping as a quality issue.

#### #63 — Review WasmKeeper Queries based on x/wasm protocol
**Why still valid:** Ethan noted in comments that response types were missing and some queries aren't implemented. The second comment (queries missing) remains the actionable part — verify against the current gRPC implementation which has a number of unimplemented stubs.

---

## Summary

| Action | Issues |
|--------|--------|
| **Close** | #144, #12, #13, #11, #49, #69, #163, #76 |
| **Keep — High priority** | #177, #178, #162, #71 |
| **Keep — Medium priority** | #117, #109, #150, #155, #148, #128, #33, #48 |
| **Keep — Low priority** | #164, #125, #124, #77, #63 |
