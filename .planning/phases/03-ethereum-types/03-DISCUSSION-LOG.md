# Phase 3: Ethereum Types - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-03
**Phase:** 03-ethereum-types
**Areas discussed:** Migration scope, Address type strategy, Transaction format depth, Storage key migration, High-level project direction

---

## High-Level Project Direction

| Option | Description | Selected |
|--------|-------------|----------|
| Stick with full migration (Phases 3+4 as planned) | Full alloy Address everywhere, then full VM replacement. Cleanest architecture, but WAVS integration delayed. | ✓ |
| Option B — surgical Phase 3, skip Phase 4 | Change encoding at boundaries only, keep CosmWasm/Wasmer, unblock WAVS integration fast. | |
| Modified approach | Something in between. | |

**User's choice:** Stick with full migration (Phases 3+4 as planned)
**Notes:** EWASM_DESIGN.md's Option B was considered and rejected.

---

## Updated Context / Priorities

| Option | Description | Selected |
|--------|-------------|----------|
| No changes — roadmap is still accurate | Phase 3 goal and success criteria are good as-is. | |
| Some things have shifted | Updated context or changed priorities to share. | ✓ |

**User's choice:** Some things have shifted
**Notes:** "Even though it's a WASM environment, we want it to support existing Ethereum wallets and standards." — Ethereum wallet compatibility (MetaMask, Rabby, ethers.js, viem) is the driving constraint for Phase 3, not just a type cleanup.

---

## Transaction Format (Key Decision)

| Option | Description | Selected |
|--------|-------------|----------|
| Full Ethereum RLP transactions | Node accepts what wallets produce natively — RLP-encoded, EIP-155 chain ID, the works. | ✓ |
| Ethereum signing, custom envelope | secp256k1 + keccak256 signing matches Ethereum, but tx body format is Layer-specific. Wallets need adapter. | |

**User's choice:** Full Ethereum RLP transactions (via "Other" response)
**Notes:** "Ideally we could support other types of signatures in the future. But Ethereum wallet support is crucial." — Extensible signing architecture desired, Ethereum-first.

---

## Summary Confirmation

| Option | Description | Selected |
|--------|-------------|----------|
| Yes, that's right | Lock decisions into CONTEXT.md and move to planning. | ✓ |
| Mostly right, but... | Correction or addition needed. | |

**User's choice:** Yes, that's right
**Notes:** Confirmed: Full alloy Address everywhere, Ethereum RLP transactions accepted natively, extensible signing (Ethereum-first), atomic RocksDB key migration where needed.

---

## Claude's Discretion

- Internal message dispatch format
- alloy crate selection strategy
- RocksDB migration implementation approach
- cosmrs removal strategy
- packages/golem handling
- Test contract updates

---

*Discussion log: 2026-04-03*
