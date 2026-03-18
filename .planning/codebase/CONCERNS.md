# Codebase Concerns

**Analysis Date:** 2026-03-18

## Tech Debt

**Unsafe Lifetime Management in WASM VM:**
- Issue: The `danger_will_robinson` function uses unsafe transmute for lifetime coercion, creating static references from borrowed pointers. This is a known footgun pattern that can lead to use-after-free if misused.
- Files: `packages/app/src/wasm/vm/backend.rs` (lines 32-58), `packages/app/src/wasm/vm/cache.rs` (lines 122, 178, 233, 288, 343, 396)
- Impact: If the unsafe contract is violated (references used after stored data is dropped), memory corruption and crashes in contract execution. Currently mitigated only by careful code review.
- Fix approach: Either refactor to use self-referential types, generics, or GATs; or formalize the safety contract with extensive testing and documentation

**Panic on Deserialization in Key Parsing:**
- Issue: Multiple `parse_keys` functions use `.unwrap()` on deserialization operations that can fail with malformed data:
  - `packages/app/src/wasm/keeper.rs:65-81` - unwraps on u64, AccountId, and tuple deserialization
  - `packages/app/src/bank/keeper.rs:33-42` - unwraps on String and AccountId deserialization
  - `packages/app/src/auth/keeper.rs:21-29` - similar pattern with unwraps
- Files: `packages/app/src/wasm/keeper.rs`, `packages/app/src/bank/keeper.rs`, `packages/app/src/auth/keeper.rs`
- Impact: Malformed keys from storage can panic the entire application. These are called during storage iteration/query operations and could be triggered by corrupted state or bugs in related code.
- Fix approach: Replace unwraps with proper error handling. Return Result types, log errors, and gracefully skip/report malformed entries.

**Test-Only Panic Statements in Production Code:**
- Issue: Bank keeper contains multiple panics in test code (lines 606, 625, 665, 676, 688, 700, etc.) that make assumptions about response types.
- Files: `packages/app/src/bank/keeper.rs:600-734` (test module)
- Impact: Tests will crash on unexpected but valid response types. Makes tests fragile and harder to debug.
- Fix approach: Replace panics with assertions that provide better error messages. Consider using pattern matching with unwrap_or_else for clearer intent.

**Hardcoded Gas Limits Without Configuration:**
- Issue: Gas limits and conversion factors are hardcoded as constants without configuration mechanism:
  - `DEFAULT_QUERY_GAS: 500_000` (app.rs:34)
  - `DEFAULT_SIMULATE_GAS: 10_000_000` (app.rs:35)
  - `SDK_TO_WASMER_GAS_FACTOR: 150_000_000` (wasm/vm/cache.rs:33)
  - Hard-coded limits in test: `BALANCES.limit.unwrap_or(100u32)` (wasm/keeper.rs:874)
- Files: `packages/app/src/app.rs`, `packages/app/src/wasm/vm/cache.rs`, `packages/app/src/wasm/keeper.rs`
- Impact: Cannot adjust gas pricing or limits without code changes and recompilation. Different nodes could have different limits if code is not synchronized.
- Fix approach: Load from genesis, store in application config, or read from a consensus state. At minimum, add `AppConfig` integration for these values.

**Unimplemented gRPC Endpoints Return Errors Instead of Panicking:**
- Issue: Many gRPC methods in slay3rd return unimplemented errors rather than actual functionality:
  - `bank.rs:93, 101, 109` - BankQuery params, denom_metadata, denoms_metadata
  - `auth.rs:36, 54, 62` - Auth accounts, params, module_account_by_name
  - `cosmwasm.rs:54, 75, 136, 145` - GetCode, GetCodeInfo, pinned_codes, params
- Files: `app/slay3rd/src/grpc/bank.rs`, `app/slay3rd/src/grpc/auth.rs`, `app/slay3rd/src/grpc/cosmwasm.rs`
- Impact: API clients expecting these endpoints to work will fail silently or with unclear errors. Documentation and client code may assume these work.
- Fix approach: Implement missing endpoints or clearly document which endpoints are not supported. Consider returning proper gRPC Status codes rather than generic unimplemented errors.

**Excessive Clone Operations:**
- Issue: 288 total `clone()` calls in packages, with many in hot paths:
  - `wasm/keeper.rs`: AccountId, BlockInfo, and event data cloned multiple times in execute/query paths
  - Multiple `clone()` on Arc-wrapped data and messages
- Files: `packages/app/src/wasm/keeper.rs` (lines 164, 193, 361-363, 620, 635-637, 665+)
- Impact: Unnecessary memory allocations and GC pressure in contract execution. Performance degradation at scale.
- Fix approach: Use references where possible, consider Cow for conditional cloning, avoid cloning in loops. Profile hot paths to identify worst offenders.

## Known Bugs

**FIXME: Bank Transfer Without Block/SM in Tests:**
- Symptoms: Comment at `packages/app/src/bank/keeper.rs:871` notes that transfer method needs block and StateMachine parameters but test doesn't provide them
- Files: `packages/app/src/bank/keeper.rs:871`
- Trigger: Running bank transfer tests or functionality
- Workaround: Test currently uses a different code path; production might hit this if refactored

**Determinism Issue in WASM Instantiate Address:**
- Symptoms: Comment at `packages/app/src/wasm/keeper.rs:1001` marks address generation as non-deterministic
- Files: `packages/app/src/wasm/keeper.rs:1001`
- Trigger: Instantiating WASM contracts - same inputs may produce different addresses across runs
- Impact: Cannot replay transactions deterministically. Critical for blockchain consensus.
- Workaround: Current implementation works but needs deterministic overhaul

**Gas Simulation Unclear for LMDB:**
- Symptoms: Comment at `js/src/bank_send.spec.ts:148` indicates simulation gas calculation is unclear for LMDB backend
- Files: `js/src/bank_send.spec.ts:148`
- Trigger: Simulating transactions on LMDB storage
- Workaround: Tests use different gas estimates but production behavior unclear

## Security Considerations

**Input Validation in Key Parsing:**
- Risk: Malformed keys could bypass deserialization and cause panics, leading to DoS attacks on the chain
- Files: `packages/app/src/wasm/keeper.rs:65-81`, `packages/app/src/bank/keeper.rs:33-42`, `packages/app/src/auth/keeper.rs:21-29`
- Current mitigation: Assumes storage provides well-formed data; relies on KeyDeserialize trait
- Recommendations:
  1. Add validation/sanitization before deserialization
  2. Consider using a custom deserializer that returns errors instead of panicking
  3. Add fuzzing tests with malformed keys
  4. Log suspicious deserialization failures for monitoring

**Unsafe Code in WASM Execution:**
- Risk: Lifetime violations in `danger_will_robinson` could allow use-after-free during concurrent contract execution
- Files: `packages/app/src/wasm/vm/backend.rs:34-58`, `packages/app/src/wasm/vm/cache.rs:69`
- Current mitigation: Documented with scary function name; reviewed manually
- Recommendations:
  1. Add SAFETY comments explaining exact guarantees
  2. Consider using miri for memory safety testing
  3. Add tests that exercise concurrent execution patterns
  4. Document which lifetimes must outlive which in inline comments

**Unimplemented Query Endpoints:**
- Risk: Clients may assume endpoints work and build logic around them; failures could lead to security-related queries being unavailable
- Files: `app/slay3rd/src/grpc/cosmwasm.rs`, `app/slay3rd/src/grpc/auth.rs`
- Current mitigation: Return errors (not panics)
- Recommendations: Implement missing endpoints or provide clear capability advertisement

## Performance Bottlenecks

**Excessive Cloning in Contract Execution:**
- Problem: AccountId, BlockInfo, and message data cloned in hot execution paths
- Files: `packages/app/src/wasm/keeper.rs:361-363, 620, 635-637` (execute path), similar patterns in query
- Cause: Rust ownership requirements, but could be optimized with references or Rc/Arc
- Improvement path:
  1. Profile execute/query to quantify overhead
  2. Use references in internal APIs where lifetime permits
  3. Consider wrapping frequently-cloned types in Arc
  4. Benchmark before/after changes

**Hardcoded Page Size Limits:**
- Problem: Max page size of 100 hardcoded in query path with TODO comment
- Files: `packages/app/src/wasm/keeper.rs:874`
- Cause: Arbitrary limit without performance testing or configuration
- Improvement path: Make configurable, benchmark with real workloads to determine optimal defaults

**Large Keeper Files:**
- Problem: Monolithic files make code harder to optimize and profile:
  - `wasm/keeper.rs`: 1474 lines
  - `bank/keeper.rs`: 1027 lines
  - Contain both logic and extensive test code
- Impact: Difficult to identify and optimize performance hot spots
- Improvement path: Split into focused modules, move tests to separate integration test files

## Fragile Areas

**Generated Protocol Buffer Code:**
- Files: `packages/proto/src/protos/*.rs` (2300+ lines of generated code)
- Why fragile: Generated code is not hand-written; regeneration can silently break if proto definitions change
- Safe modification: Never hand-edit generated proto files. Always edit .proto source and regenerate with build script.
- Test coverage: Gaps - generated code is not directly tested; tested only through usage
- Recommendation: Add proto compilation test to catch breaking changes early

**Storage Layer Abstraction:**
- Files: `packages/storage/src/rocks/mod.rs` (692 lines), `packages/storage/src/plus/map.rs` (1818 lines)
- Why fragile: Low-level storage operations could cause data corruption if bounds/serialization is wrong
- Safe modification: All storage changes need integration tests with actual persistence
- Test coverage: Limited coverage of error cases and edge conditions
- Recommendation: Add property-based testing for storage operations

**Parse Keys Functions with Panics:**
- Files: `packages/app/src/wasm/keeper.rs:65-81`, `packages/app/src/bank/keeper.rs:33-42`
- Why fragile: Any storage key corruption causes immediate crash instead of graceful degradation
- Safe modification: Never assume key format is valid; always validate before deserializing
- Test coverage: Gaps - no tests with malformed keys
- Recommendation: Add fuzz tests and integration tests with corrupted keys

**WASM Contract Execution via Unsafe:**
- Files: `packages/app/src/wasm/vm/cache.rs` (7 unsafe blocks)
- Why fragile: Lifetime bugs could silently cause use-after-free in production
- Safe modification: Do not modify unsafe blocks without deep understanding of lifetime rules. Add extensive comments.
- Test coverage: Gaps - no concurrent execution tests or miri memory safety tests
- Recommendation: Add concurrent stress tests and run under miri

## Scaling Limits

**Gas Meter Precision:**
- Current: Gas calculated and checked at application level
- Limit: SDK_TO_WASMER_GAS_FACTOR hardcoded at 150,000,000; conversion may lose precision at high gas values
- Scaling path: Profile actual gas consumption with realistic contracts; adjust factor or use higher precision types if needed

**Storage Iteration Performance:**
- Current: Range queries implemented in `storage/src/plus/map.rs`
- Limit: No pagination or limiting in storage layer; client must implement
- Scaling path: Add storage-level pagination to avoid loading entire result sets into memory

**Consensus State:**
- Current: All application state in single process
- Limit: No sharding or state partitioning
- Scaling path: Consider state sharding strategy if blockchain grows beyond single-machine capacity

## Dependencies at Risk

**cosmwasm-vm 1.5.4:**
- Risk: Fixed old version; CosmWasm ecosystem moving forward. May have unpatched vulnerabilities.
- Impact: Contract execution bugs, security issues in WASM sandbox
- Migration plan: Plan quarterly upgrades to track cosmwasm-vm releases. Test with contract suite before upgrading.

**Tendermint 0.39.1:**
- Risk: Matches specific tendermint-proto and tendermint-rpc versions. Version lock could miss security patches.
- Impact: P2P networking bugs, consensus issues
- Migration plan: Monitor tendermint releases; coordinate upgrade with all nodes

**RocksDB (indirect via rocks crate):**
- Risk: Storage dependency; breaking changes could require migration
- Impact: Inability to start nodes if RocksDB format incompatibility occurs
- Migration plan: Test storage migrations before upgrading; maintain changelog of format changes

## Missing Critical Features

**Pagination Limits in gRPC Endpoints:**
- Problem: Tendermint gRPC responses missing pagination info
- Blocks: Clients cannot properly paginate large result sets
- Files: `app/slay3rd/src/grpc/tendermint.rs:75` (marked as TODO)

**Build Information in Binary:**
- Problem: Build metadata incomplete in VersionInfo
- Blocks: Debuggability; cannot determine build source from running binary
- Files: `app/slay3rd/src/grpc/tendermint.rs:117-119` (build_tags and build_deps as TODO)

**Query Capabilities Advertisement:**
- Problem: No way for clients to query which gRPC endpoints are supported
- Blocks: Clients must try all endpoints to discover support
- Recommendation: Add capabilities query or service description endpoint

**Permissions System for Root Contract:**
- Problem: Root contract has all-or-nothing permissions model
- Blocks: Fine-grained access control
- Files: `contracts/root/src/msg.rs:93, 97` (marked TODO for partial privileges)

**Gas Limit Configuration:**
- Problem: Hard limits not configurable per-node
- Blocks: Consensus-breaking if nodes have different limits
- Files: `packages/app/src/app.rs:39-41` (marked as FIXME)

## Test Coverage Gaps

**Key Deserialization Errors:**
- What's not tested: Malformed keys passed to parse_keys functions
- Files: `packages/app/src/wasm/keeper.rs:65-81`, `packages/app/src/bank/keeper.rs:33-42`
- Risk: Corruption or attack via malformed keys causes panics
- Priority: High

**Unsafe WASM Execution Paths:**
- What's not tested: Concurrent contract execution; lifetime safety under stress
- Files: `packages/app/src/wasm/vm/cache.rs`, `packages/app/src/wasm/vm/backend.rs`
- Risk: Use-after-free, memory corruption in production with concurrent contracts
- Priority: High

**Storage Corruption Scenarios:**
- What's not tested: Database corruption recovery; behavior with partially-written data
- Files: `packages/storage/src/rocks/mod.rs`
- Risk: Unrecoverable chain state if storage is corrupted
- Priority: Medium

**gRPC Error Handling:**
- What's not tested: Client behavior with unimplemented endpoints
- Files: `app/slay3rd/src/grpc/bank.rs`, `app/slay3rd/src/grpc/auth.rs`
- Risk: Clients may hang or behave incorrectly on 'unimplemented' errors
- Priority: Medium

---

*Concerns audit: 2026-03-18*
