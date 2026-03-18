# Testing Patterns

**Analysis Date:** 2026-03-18

## Test Framework

**TypeScript Test Runner:**
- Framework: **Jasmine** 5.0.0
- Config file: `js/jasmine.json`
- Custom test runner: `js/jasmine-testrunner.js`
- Reporter: `jasmine-spec-reporter` 7.0.0

**Run Commands:**
```bash
npm run build              # Compile TypeScript to build/
npm run test               # Run all tests (build then execute jasmine-testrunner.js)
npm run ts-test            # Run tests with ts-node (direct TypeScript)
npm run lint               # Run ESLint with max-warnings 0
npm run lint-fix           # Fix linting issues
npm run format             # Format code with Prettier
```

**Rust Test Runner:**
- Framework: Built-in Rust test framework (`cargo test`)
- Test orchestration: `cw-orch` for contract testing with `MockBech32` and live chain environments
- Config: Cargo.toml workspace settings

**Run Commands (Rust):**
```bash
cargo test                 # Run all tests in workspace
cargo test --package {package-name}  # Run specific package tests
```

## Test File Organization

**TypeScript Location:**
- Pattern: Co-located with source - `src/**/*.spec.ts`
- Example files:
  - `js/src/abstract.spec.ts`
  - `js/src/bank_send.spec.ts`
  - `js/src/cw20.spec.ts`
  - `js/src/testutils.spec.ts` (shared utilities)

**Naming:**
- Suffix pattern: `.spec.ts` (Jasmine convention, not `.test.ts`)
- Jasmine config: `spec_files: ["*spec.ts"]` in `jasmine.json`

**Rust Location:**
- Pattern: Inline with `#[cfg(test)]` modules or separate `tests/` directory
- Example: `contracts/root/src/contract.rs` has inline `#[cfg(test)] mod tests {}`
- Shared utilities: `contracts/root/src/tests/common.rs` (shared setup helpers)

**Structure:**
```
js/src/
├── bank_send.spec.ts      # Test suite for bank operations
├── cw20.spec.ts           # Test suite for CW20 tokens
├── abstract.spec.ts       # Test suite for abstract manager
├── testutils.spec.ts      # Shared test fixtures and utilities
└── utils.ts               # Production code

contracts/root/src/
├── contract.rs            # Implementation with inline #[cfg(test)] module
├── tests/
│   ├── mod.rs             # Test module declarations
│   ├── common.rs          # Shared setup and utilities
│   └── multi.rs           # Multi-contract scenario tests
└── error.rs               # Error definitions
```

## Test Structure

**Jasmine Test Suite Pattern:**

```typescript
describe("SigningStargateClient", () => {
  describe("simulate", () => {
    it("works", async () => {
      // Arrange
      const wallet = await DirectSecp256k1HdWallet.fromMnemonic(
        faucet.mnemonic,
        defaultWalletOptions
      );
      const cometClient = await Comet38Client.connect(localNet.tendermintUrl);
      const client = await SigningStargateClient.createWithSigner(
        cometClient,
        wallet,
        defaultSigningClientOptions
      );

      // Act
      const msg = MsgSend.fromPartial({...});
      const gasUsed = await client.simulate(faucet.address0, [msgAny], memo);

      // Assert
      expect(gasUsed).toBeGreaterThanOrEqual(3_000);
      expect(gasUsed).toBeLessThanOrEqual(60_000);

      // Cleanup
      client.disconnect();
    });
  });
});
```

**Patterns:**
- **Setup:** Create clients, wallets, test data in `beforeEach` or test body
- **Teardown:** Call `client.disconnect()` explicitly
- **Assertions:** Jasmine matchers (toBeTruthy, toEqual, toBeGreaterThan, etc.)

**Rust Test Pattern (cw-orch):**

```rust
#[test]
fn happy_path_works() {
    let chain = MockBech32::new(BECH_PREFIX);
    super::common::happy_path(chain);
}
```

**Shared Setup Utility Pattern:**

```rust
pub fn setup<Chain: CwEnv>(chain: Chain, msg: InstantiateMsg) -> Contract<Chain> {
    let contract = Contract::new(chain);
    contract.upload().unwrap();
    contract.instantiate(&msg, None, None).unwrap();
    contract
}
```

## Mocking

**TypeScript Mocking:**
- No explicit mocking framework detected (Jest/Sinon not used)
- Custom mock implementations created as classes:
  - `ModifyingSecp256k1HdWallet` - Extends `Secp256k1HdWallet` to intercept and modify transactions
  - `ModifyingDirectSecp256k1HdWallet` - Extends `DirectSecp256k1HdWallet` for direct signer mocking

**Custom Mock Example:**
```typescript
export class ModifyingDirectSecp256k1HdWallet extends DirectSecp256k1HdWallet {
  public static override async fromMnemonic(
    mnemonic: string,
    options: Partial<DirectSecp256k1HdWalletOptions> = {}
  ): Promise<DirectSecp256k1HdWallet> {
    const mnemonicChecked = new EnglishMnemonic(mnemonic);
    const seed = await Bip39.mnemonicToSeed(mnemonicChecked, options.bip39Password);
    return new ModifyingDirectSecp256k1HdWallet(mnemonicChecked, { ...options, seed: seed });
  }

  public override async signDirect(
    signerAddress: string,
    signDoc: SignDoc
  ): Promise<DirectSignResponse> {
    // Modify the transaction before signing
    const modifiedSignDoc = {...};
    return super.signDirect(signerAddress, modifiedSignDoc);
  }
}
```

**Rust Mocking:**
- No explicit mocking framework used
- Use `MockBech32` from `cw-orch` for contract testing
- Integration testing with test chain environments

**What to Mock:**
- External client connections (handled via constructor injection)
- Wallet signers (for testing different signing scenarios)
- Chain environments (MockBech32 for unit tests, live chains for integration)

**What NOT to Mock:**
- Business logic (test actual contract code)
- Underlying CosmJS client methods (integration tests verify interactions)
- Blockchain state (use test fixtures and known addresses)

## Fixtures and Factories

**Test Data (TypeScript):**

Shared utilities in `js/src/testutils.spec.ts`:

```typescript
export const PREFIX = "layer";
export const DENOM = "uslay";

export const defaultGasPrice = GasPrice.fromString("0.025" + DENOM);
export const defaultSendFee = calculateFee(100_000, defaultGasPrice);

export const localNet = {
  tendermintUrl: `http://localhost:26657`,
  chainId: "slay3r-local",
  blockTime: 1_000,
  totalSupply: 21000000000,
};

export const faucet = {
  mnemonic: "economy stock theory fatal...",
  address0: "layer1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmt53rug",
  address1: "layer10dyr9899g6t0pelew4nvf4j5c3jcgv0rwnkg3x",
  // ... more addresses
};

export function makeRandomAddress(): string {
  return toBech32(PREFIX, makeRandomAddressBytes());
}
```

**Fixtures Location:**
- Constants: `js/src/testutils.spec.ts`
- WASM binaries: `js/testdata/` (todo_list.wasm, etc.)
- Contract fixtures: `packages/app/fixtures/` (cw20_base.wasm, abstract_manager.wasm)

**Factory Functions:**
- `makeRandomAddress()` - Generate random test addresses
- `makeRandomAddressBytes()` - Create random 20-byte sequences
- `fromOneElementArray<T>()` - Assert and extract single array element
- Custom wallet factories: `DirectSecp256k1HdWallet.fromMnemonic()`

**Rust Test Fixtures:**

```rust
pub const BECH_PREFIX: &str = "slay3r";

pub fn setup<Chain: CwEnv>(
    chain: Chain,
    msg: InstantiateMsg
) -> Contract<Chain> {
    let contract = Contract::new(chain);
    contract.upload().unwrap();
    contract.instantiate(&msg, None, None).unwrap();
    contract
}
```

## Coverage

**Requirements:** No explicit coverage target detected in configuration

**Jasmine Config Settings:**
```json
{
  "env": {
    "failSpecWithNoExpectations": false,
    "stopSpecOnExpectationFailure": true,
    "stopOnSpecFailure": true,
    "random": false
  }
}
```

**View Coverage:**
- No coverage tool configured (Istanbul, Nyc, etc. not in dependencies)
- To add coverage: Would require installing `karma-coverage` and configuring jasmine runner

## Test Types

**Unit Tests:**
- Scope: Individual contract functions and utilities
- Approach: Test contract endpoints with mocked blockchain state
- Example: `contracts/echo/src/contract.rs` - Tests counter increment, echo response
- Isolation: Use MockBech32 for isolated execution

**Integration Tests:**
- Scope: Client-to-blockchain interaction (TypeScript), contract instantiation (Rust)
- Approach: Full contract lifecycle (upload, instantiate, execute, query)
- Example: `js/src/cw20.spec.ts` - Full CW20 workflow (upload → instantiate → transfer → query)
- Live Chain: Tests connect to running `localhost:26657` tendermint RPC

**E2E Tests:**
- Framework: Not formally organized, but integration tests serve this purpose
- Local network: `js/src/` tests assume local slay3r node running
- Validators: Test uses known faucet addresses and transactions

**Contracts Tests:**
- Framework: `cw-orch` with `MockBech32` and `CwEnv` trait
- Example: `contracts/root/src/tests/multi.rs` - Multi-contract scenarios
- Location: Inline with `#[test]` attribute

## Common Patterns

**Async Testing (TypeScript):**
```typescript
it("works", async () => {
  const wallet = await DirectSecp256k1HdWallet.fromMnemonic(
    faucet.mnemonic,
    defaultWalletOptions
  );
  const cometClient = await Comet38Client.connect(localNet.tendermintUrl);
  // ... rest of test
});
```

- All tests are async due to blockchain interaction
- Jasmine handles async tests via `async`/`await`
- No explicit timeout management (default: 15 seconds per `jasmine-testrunner.js`)

**Error Testing (TypeScript):**
```typescript
it("returns DeliverTxFailure on DeliverTx failure", async () => {
  // ... setup ...
  const result = await client.signAndBroadcast(faucet.address0, [msgAny], fee);
  assertIsDeliverTxFailure(result);
  expect(result.code).toBeGreaterThan(0);
  expect(result.rawLog).toMatch(/insufficient funds/);
});
```

- Use type guards: `assertIsDeliverTxSuccess()`, `assertIsDeliverTxFailure()`
- Check error codes and messages via `.code` and `.rawLog`
- Match error messages with regex patterns

**Error Testing (Rust):**
```rust
pub fn happy_path<C>(chain: C)
where
    C: CwEnv + AltSigner,
    C::Sender: Addressable,
{
    let msg = InstantiateMsg {};
    let contract = setup(chain.clone(), msg);

    let code_id = contract.code_id().unwrap();
    assert_eq!(code_id, 1);
}
```

- Use `.unwrap()` to assert success in tests
- Use `assert_eq!()` and `assert!()` for assertions
- Contract errors propagate via `Result` type

**Resource Cleanup:**
- TypeScript: Explicit `client.disconnect()` calls
- Rust: Automatic via ownership/drop semantics

**Test Configuration:**
- Timeouts: 15 seconds per test (jasmine-testrunner.js)
- Stop on failure: `stopSpecOnExpectationFailure: true`
- Random order disabled: `random: false`
- Suppress passing specs output: `--quiet` flag suppresses success output

---

*Testing analysis: 2026-03-18*
