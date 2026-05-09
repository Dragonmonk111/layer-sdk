# EWASM Design

Design notes and pseudo-code examples for EWASM — Layer's stateful contract system.

EWASM contracts blend CosmWasm's actor model with EVM types and ABI encoding. They look like Rust, dispatch like Solidity, and manage state like CosmWasm.

## Files

| File | Description |
|------|-------------|
| [sdk-surface.md](sdk-surface.md) | SDK primitives — types, traits, storage, response |
| [example-counter.md](example-counter.md) | Hello world counter contract |
| [example-operator-registry.md](example-operator-registry.md) | Operator registration, staking, slashing |
| [example-task-mailbox.md](example-task-mailbox.md) | WAVS-to-Layer bridge — task submission and quorum finalization |
| [design-decisions.md](design-decisions.md) | Rationale for key design choices |
