# JunoClaw Governance & Treasury Plan

Last updated: 2026-10-04

## The problem this solves

The original `snapshot/build-genesis.mjs` shipped with a hardcoded default:
**all 54,660,000 ujclaw and `wasm.gov_account` pointed at
`juno18k65at7fkf8elhece0fnhsvuxggqg6cved6trp5fyk3lftfn93xsmpeaac`** — a
juno-1 **DAO DAO contract** address (63 chars, 32-byte bech32 payload).

Nobody holds a private key for that address on junoclaw-chain. Shipping that
genesis would have locked the entire supply and every contract-admin /
root-call capability forever. Fixed in build-genesis.mjs:

- `--dao` is now **required**, no default.
- A bech32 payload-length guard **rejects any 32-byte address** (contract/ICA)
  for `--dao`, `--treasury`, and `--gov`. Only 20-byte key-controlled accounts
  pass.
- `--airdrop-file` reads the claimable total from `merkle-proofs.json`, which
  now excludes the 114 contract/ICA snapshot recipients (859,972.98 JCLAW —
  routed to the community pool, see `merkle-proofs.excluded.json`).

## Why a contract, not a native multisig, for G2+

junoclaw-chain's `layer-std`/hybrid account model signs with secp256k1 +
MAYO-2/3/5. There is **no `cosmos.crypto.multisig.LegacyAminoPubKey`** path in
the tx decoder — multisignature accounts are not a chain primitive. Options
considered:

- **Native x/auth multisig** — not implemented; adding it is a consensus-layer
  change with real audit surface. Rejected for launch.
- **juno-1 DAO DAO contract as gov_account at genesis** — requires genesis
  wasm instantiation of a contract whose own code must already exist;
  self-referential and fragile. Rejected.
- **Key-controlled account now, multisig *contract* later** — works today:
  contracts get their admin from `WasmQuery::ContractInfo` / instantiate
  sender, and `gov_account` can be pointed at any address via a future
  upgrade. This is the plan.

## Phases

### G1 (now — devnet + validator onboarding)

- `--dao` = the deterministic devnet deployer
  `juno1dz875zg8p78anpjv3f0qt4gu5a3awpjfhtw992`
  (SHA256("junoclaw-deployer-v1") → secp256k1). Used in `snapshot/genesis.json`.
- `wasm.gov_account` = same address. It is the contract-admin / root caller.
- Single-operator custody is **acceptable for devnet only**.

### Devnet deployment (live-verified 2026-10-04)

`junoclaw-dao` lock-to-vote contract (stake→lock rename; vote weight comes
from the `LOCKED` map, not the live bank balance — blocks the
transfer-revote attack):

- **code_id 4** — `data_hash 94a9d5590aed4a50454887b3756d0875c3e994f027357c0edcd713f0acbf10de`
- **contract** `juno1tsgw7mek36kge5kw2e8jwg06u0lkwc0ehl0vfp97v5lynzslvwzqq9hfs2`
  (instantiated h276887, tx `266CFAA0…`; init params in `snapshot/dao-init.json`)
- E2E verified: `Lock{}` + 500M ujclaw → `GetLock` = 500000000; `Vote` yes →
  `GetTally.yes_votes` = 500000000; `Unlock` mid-vote correctly rejected
  (`Tokens locked until block 278796`); `GetLockStats.spendable` = 0.
- Serialization notes: unit enum variants take bare strings
  (`"proposal_type":"text"`, `"vote":"yes"`); `u128` fields take JSON
  numbers; use `tx-sender --msg-file` (PowerShell mangles inline JSON).

### G1 ceremony (testnet genesis bundle)

- A **ceremony key** is generated at the key ceremony (offline, split custody:
  seed phrase split across ≥3 operators, or a hardware-keyed account operated
  by the coordinating member). Pass it as `--dao`; `gov_account` inherits it.
- This is interim — it exists to bootstrap, then hands off to the DAO.

### G2 (testnet → mainnet hardening)

- Deploy the **multisig contract**. Two candidate paths:
  1. `junoclaw-dao` lock-to-vote contract (already audited for the
     transfer-revote fix) — token-weighted spend proposals out of the
     community pool. Good for treasury allocation decisions.
  2. A minimal **fixed-membership multisig** (cw3-flex-style, N-of-M signer
     set) for ops-sensitive actions (contract admin, param changes). This is
     what `gov_account` should eventually point at.
- Handoff sequence:
  1. Deploy multisig contract, funded with a dust amount.
  2. Ceremony key submits a chain upgrade / param tx to set
     `gov_account` = multisig contract address.
  3. Transfer treasury from ceremony key to the multisig / DAO contract.
  4. Ceremony key is retired to cold storage (kept as emergency breakglass
     until G2+2 epochs of clean operation).

### Airdrop claim (parallel, G1+)

- Deploy `airdrop-claim` with merkle root
  `d38cfb7aa55d2f45babca83cc4b7c426b9fcf50e998eb74613cb9ee30badb4e2`.
- Transfer **34,180,002.099594 JCLAW** (claimable total) from treasury to it.
- 90-day claim window; `sweep_unclaimed` returns the remainder to the
  community pool.

## Invariants (CI-checkable)

1. No genesis address may decode to a 32-byte payload. (Enforced in
   build-genesis.mjs; also worth a pre-merge CI step that runs the script on
   the real snapshot and fails on non-zero exit.)
2. `bank` balances in genesis.json must sum to exactly 54,660,000,000,000
   ujclaw.
3. `wasm.gov_account` must appear in `bank` (the gov key should be funded).
4. `merkle_root` in `merkle-proofs.json` must equal the root baked into the
   airdrop-claim instantiate msg — verify before funding the contract.
