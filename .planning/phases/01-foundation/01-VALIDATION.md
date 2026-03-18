---
phase: 1
slug: foundation
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-18
---

# Phase 1 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (built-in Rust test harness) |
| **Config file** | none — uses `[profile.test]` in root `Cargo.toml` |
| **Quick run command** | `cargo test -p layer-app --lib 2>&1 \| tail -5` |
| **Full suite command** | `cargo test --workspace --lib 2>&1` |
| **Estimated runtime** | ~30 seconds (unit tests only; miri adds ~2 min) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p layer-app --lib 2>&1 | tail -5`
- **After every plan wave:** Run `cargo test --workspace --lib 2>&1`
- **Before `/gsd:verify-work`:** `cargo build --workspace` + `cargo test --workspace --lib` + `cargo miri test -p layer-app --lib -- wasm::vm` must all be green
- **Max feedback latency:** ~30 seconds (quick run)

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 1-01-01 | 01 | 1 | FOUND-01 | build smoke | `cargo build --workspace` | N/A | ⬜ pending |
| 1-01-02 | 01 | 1 | FOUND-01 | audit | `cargo audit` | N/A | ⬜ pending |
| 1-02-01 | 02 | 1 | FOUND-02 | miri | `cargo miri test -p layer-app --lib -- wasm::vm` | ❌ W0 | ⬜ pending |
| 1-02-02 | 02 | 1 | FOUND-02 | unit | `cargo test -p layer-app --lib -- wasm::vm::cache` | ✅ | ⬜ pending |
| 1-03-01 | 03 | 1 | FOUND-03 | unit | `cargo test -p layer-app --lib -- wasm::utils::test::counter_address_is_deterministic` | ❌ W0 | ⬜ pending |
| 1-03-02 | 03 | 1 | FOUND-03 | unit | `cargo test -p layer-app --lib -- wasm::utils` | ✅ | ⬜ pending |
| 1-04-01 | 04 | 2 | FORK-01 | build smoke | `cargo build -p cosmwasm-vm` | ❌ W0 (needs submodule) | ⬜ pending |
| 1-04-02 | 04 | 2 | FORK-02 | compile | `cargo check -p layer-app` | ❌ W0 (needs fork) | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Install `cargo-audit`: `cargo install cargo-audit`
- [ ] Install `miri` toolchain: `rustup component add miri`
- [ ] New test file stubs: `packages/app/src/wasm/utils.rs` — add `counter_address_is_deterministic` test for FOUND-03

*All other test infrastructure already exists via `#[cfg(test)]` modules in the packages.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| CosmWasm fork submodule points to correct commit | FORK-01 | Requires visual git inspection | Run `git submodule status` and verify SHA matches fork's v2.3.2+CWA-2024-004 commit |
| CWA-2024-004 gas constants present in fork | FORK-01 | Requires reading fork source | Check `cosmwasm/packages/vm/src/wasm_backend/engine.rs` for `GAS_PER_OPERATION = 115` and 14x multiplier for control-flow ops |
| BackendApi stubs use `unimplemented!()` not `todo!()` | FORK-02 | Convention check | Read `cosmwasm/packages/vm/src/backend.rs` and verify stub methods contain `unimplemented!("... not implemented — Phase 4")` |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
