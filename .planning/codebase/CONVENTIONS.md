# Coding Conventions

**Analysis Date:** 2026-03-18

## Naming Patterns

**Files (TypeScript):**
- Test files: `*.spec.ts` or `*.test.ts` (e.g., `bank_send.spec.ts`, `abstract.spec.ts`)
- Source files: `camelCase.ts` (e.g., `utils.ts`, `testutils.spec.ts`)
- Config files: lowercase with hyphens (e.g., `jasmine-spec-reporter.config.json`)

**Files (Rust):**
- Source files: `snake_case.rs` (e.g., `contract.rs`, `config.rs`, `main.rs`)
- Module files: `mod.rs` for module declarations
- Binary files: `bin/*.rs` (e.g., `schema.rs`)

**Functions:**
- **TypeScript:** `camelCase` for all functions - `makeRandomAddress()`, `fromOneElementArray()`, `mnemonicToAddr()`
- **Rust:** `snake_case` for all functions - `setup()`, `happy_path()`, `incr_counter()`

**Variables:**
- **TypeScript:** `camelCase` for all variables and constants
  - Constants: Uppercase when exported (e.g., `PREFIX`, `DENOM`)
  - Regular variables: camelCase (e.g., `faucet`, `signer`, `wallet`)
- **Rust:** `snake_case` for variables and `CONSTANT_CASE` for constants
  - Example: `const CONTRACT_NAME: &str = "crates.io:layer-root"`
  - Items: `const COUNTER: Item<u64> = Item::new("counter")`

**Types:**
- **TypeScript:** `StrictPascalCase` enforced by ESLint
  - Example: `DirectSecp256k1HdWallet`, `SigningStargateClient`
- **Rust:** `PascalCase` for structs and types
  - Example: `ContractError`, `InstantiateMsg`, `RawConfig`

**Enum Members:**
- **TypeScript:** `StrictPascalCase` (enforced by ESLint naming-convention rule)
- **Rust:** `PascalCase` enforced

**Object Literal Keys:**
- Flexible to support domain-specific naming (e.g., type URLs `/cosmos.feegrant.v1beta1.MsgGrantAllowance`, test data `0.14ucoin2`)

## Code Style

**Formatting (TypeScript):**
- Tool: **Prettier** (no explicit `.prettierrc` - uses defaults)
- Enforced by ESLint with `eslint-config-prettier` to prevent conflicts
- Line width: Default (80 chars implied)
- Quotes: Double quotes (implicit from CosmJS patterns)

**Linting (TypeScript):**
- Tool: **ESLint** 8.41.0
- Config: `js/.eslintrc.js` (master copy synced to subdirectories)
- Extends: `eslint:recommended`, `@typescript-eslint/recommended`, `prettier`
- Max warnings: 0 (strict enforcement with `--max-warnings 0`)

**Key ESLint Rules:**
- `curly`: `["warn", "multi-line", "consistent"]` - Braces required for multi-line, consistent style
- `no-console`: `["warn", { allow: ["error", "info", "table", "warn"] }]` - Only these console methods allowed
- `prefer-const`: `warn` - Prefer const over let
- `@typescript-eslint/explicit-function-return-type`: `["warn", { allowExpressions: true }]` - Return types required
- `@typescript-eslint/explicit-member-accessibility`: `warn` - Public/private keywords required
- `@typescript-eslint/no-explicit-any`: `off` - Any is allowed
- `@typescript-eslint/no-empty-function`: `off` - Empty functions allowed
- `@typescript-eslint/no-empty-interface`: `off` - Empty interfaces allowed
- `@typescript-eslint/no-unused-vars`: `["warn", { argsIgnorePattern: "^_", varsIgnorePattern: "^_" }]` - Unused vars as warnings unless prefixed with `_`

**Formatting (Rust):**
- Tool: Built-in `rustfmt` (default formatting)
- No custom `rustfmt.toml` found - uses Rust edition 2021 defaults
- Clippy linting: Standard Rust conventions (no custom config)

**Key TypeScript Compiler Options:**
- Target: `es2020`
- Module: `commonjs`
- Strict mode: `true`
- `noImplicitReturns`: `true`
- `noImplicitOverride`: `true`
- `forceConsistentCasingInFileNames`: `true`
- Declaration files: Generated (`declaration: true`)

## Import Organization

**TypeScript Order:**
1. External libraries from `@cosmjs`, `cosmjs-types`, etc.
2. Node.js built-ins (e.g., `fs`, `path`)
3. Local imports (relative paths like `./testutils.spec`)

**Pattern Example:**
```typescript
import { SigningCosmWasmClient } from "@cosmjs/cosmwasm-stargate";
import { DirectSecp256k1HdWallet } from "@cosmjs/proto-signing";
import fs from "fs";

import { defaultSigningClientOptions, faucet } from "./testutils.spec";
```

**Plugins:**
- `simple-import-sort`: Enforces import sorting (warnings)
- `import/no-cycle`: Detects circular imports (warnings)

**Rust Pattern:**
- Standard Rust module system with `use` statements
- Example: `use cosmwasm_std::ensure_eq`
- Local module imports: `use crate::error::ContractError`

## Error Handling

**TypeScript Patterns:**
- Use direct `throw` statements: `throw new Error(\`Expected exactly one element but got ${elements.length}\`)`
- Cosmos client utilities provide typed responses with assertions
  - Example: `assertIsDeliverTxSuccess(result)` and `assertIsDeliverTxFailure(result)`
- Type guards for result validation before accessing fields

**Rust Patterns:**
- **Custom Error Types:** Use `thiserror` crate with `#[derive(Error, Debug)]`
  - File: `contracts/root/src/error.rs` shows standard pattern
  - Return type: `Result<T, ContractError>`

**Example Rust Error Definition:**
```rust
#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Already Registered for begin/end blocker")]
    AlreadyRegistred,
}
```

- Use `?` operator for error propagation
- Use `ok_or()` to convert `Option` to `Result`
- Use `ensure_eq!()` macro for equality checks with custom error types

## Logging

**TypeScript Framework:** `console` API
- Allowed methods: `error`, `info`, `warn`, `table`
- Typical patterns:
  - `console.info(\`Wasm size: ${wasm.length} bytes\`)`
  - `console.error("Please provide address")`
  - `console.log(\`Sent ${coin} from ${faucetAddr} to ${addr}\`)` (Note: `log` not in approved list but used)

**Rust Framework:** `tracing` crate (v0.1.37)
- Log levels: `info!()`, `error!()`, `warn!()`, `debug!()` (as macros)
- Integration: Supports structured logging with `tracing-subscriber`
- Example: `info!("jaeger tracing enabled")`
- Supports OpenTelemetry integration via `tracing-opentelemetry`

**Configuration:**
- TypeScript: No centralized logging setup in tests
- Rust (slay3rd): Configured via environment with `EnvFilter`
  - File: `app/slay3rd/src/config.rs` - configurable log levels

## Comments

**When to Comment:**
- Document non-obvious business logic or constraints
- Explain workarounds and known issues (prefix with `TODO:`, `FIXME:`, or `HACK:`)
- Comments in test files are sparse; tests should be self-documenting

**Examples Found:**
- `// TODO: more realistic gas estimate (something not measured here)`
- `// FIXME: put real message here`
- `// not sure if this is needed but something is odd here (always out of gas)`

**JSDoc/TSDoc:**
- Minimal usage in the codebase
- Some functions documented with inline comments
- Return types are enforced by TypeScript, reducing need for documentation

## Function Design

**Size:**
- Functions are kept relatively small and focused
- Test setup functions: `setup()`, `happy_path()` are reusable across multiple tests

**Parameters:**
- TypeScript: Explicit types required by strict TypeScript
- Rust: Type signatures enforced at compile time
- Use of configuration objects/structs for multiple related parameters

**Return Values:**
- TypeScript: Explicit return types on all functions (except expressions)
- Rust: Explicit `Result<T, E>` for fallible operations
- Use `Option<T>` for optional values

**Example TypeScript Function:**
```typescript
export function fromOneElementArray<T>(elements: ArrayLike<T>): T {
  if (elements.length !== 1) throw new Error(`Expected exactly one element but got ${elements.length}`);
  return elements[0];
}
```

**Example Rust Function:**
```rust
pub fn setup<Chain: CwEnv>(chain: Chain, msg: InstantiateMsg) -> Contract<Chain> {
    let contract = Contract::new(chain);
    contract.upload().unwrap();
    contract.instantiate(&msg, None, None).unwrap();
    contract
}
```

## Module Design

**Exports:**
- **TypeScript:** Explicit exports using `export const`, `export function`, `export class`
- **Rust:** Explicit exports with `pub` keyword at module level
  - Example: `pub const PREFIX = "layer"`, `pub fn setup<Chain: CwEnv>(...)`

**Barrel Files:**
- Some use of re-exports for convenience
- Example: `contracts/root/src/lib.rs` re-exports public items from submodules

**Module Organization:**
- Functions grouped by domain (e.g., all setup utilities together)
- Config as separate modules in Rust (e.g., `config.rs`)
- Tests co-located with implementation using `#[cfg(test)]` or separate test files

---

*Convention analysis: 2026-03-18*
