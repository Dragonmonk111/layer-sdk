---
phase: 2
slug: commonware-consensus
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-19
---

# Phase 2 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust) |
| **Config file** | Cargo.toml (workspace root) |
| **Quick run command** | `cargo test -p slay3rd 2>&1 | tail -20` |
| **Full suite command** | `cargo test --workspace 2>&1 | tail -40` |
| **Estimated runtime** | ~60 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p slay3rd 2>&1 | tail -20`
- **After every plan wave:** Run `cargo test --workspace 2>&1 | tail -40`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 120 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 2-01-01 | 01 | 0 | CONS-01 | unit | `cargo test -p slay3rd test_layer_node` | ❌ W0 | ⬜ pending |
| 2-01-02 | 01 | 1 | CONS-01 | integration | `cargo test -p slay3rd test_automaton_trait` | ❌ W0 | ⬜ pending |
| 2-01-03 | 01 | 1 | CONS-02 | integration | `cargo test -p slay3rd test_consensus_determinism` | ❌ W0 | ⬜ pending |
| 2-02-01 | 02 | 0 | CONS-03 | unit | `cargo test -p slay3rd test_dkg_setup` | ❌ W0 | ⬜ pending |
| 2-02-02 | 02 | 1 | CONS-04 | integration | `cargo test -p slay3rd test_wal_recovery` | ❌ W0 | ⬜ pending |
| 2-03-01 | 03 | 2 | CONS-05 | unit | `cargo test -p slay3rd test_bls_certificate` | ❌ W0 | ⬜ pending |
| 2-03-02 | 03 | 2 | CONS-05 | integration | `cargo test -p slay3rd test_certificate_verification` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `slay3rd/src/lib.rs` — add `pub mod tests` with stubs for LayerNode, Automaton trait impl
- [ ] `slay3rd/src/layer_node.rs` — initial skeleton struct (empty impl)
- [ ] `slay3rd/tests/consensus_integration.rs` — integration test stubs for 3-node testnet scenario
- [ ] `slay3rd/tests/dkg_setup.rs` — DKG key generation helper stubs
- [ ] DKG key generation tooling must be available before Wave 1 begins

*Wave 0 is mandatory: no consensus tests exist yet and all RESEARCH tasks require DKG keys as prerequisite.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| 3-node testnet reaches consensus and all nodes produce identical AppHash | CONS-02 | Requires running 3 separate processes + network | Start 3 nodes with `./scripts/testnet.sh`, wait 10 blocks, compare AppHash logs |
| Crashed node rejoins testnet from WAL without manual intervention | CONS-04 | Requires process kill + restart + observation | Kill node 2 at block 5, restart at block 10, verify it catches up to block 15+ |
| BLS certificate is verifiable by offline verifier | CONS-05 | Requires extracting certificate bytes + offline tool | Export block header, run `cargo run --bin verify-cert -- <block-hash>` |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
