# DAO DAO P0 — devnet/testnet deploy runbook

Companion to `DAO-ON-JUNOCLAW.md` (the pitch). This is the executable
checklist for the P0 audit: store the suite, instantiate a minimal DAO
with the deepest nesting path (core → voting → token+staking, core →
proposal), record every gap.

## 0. Artifacts (downloaded + checksum-verified)

DAO DAO `v2.7.1` release + cw-plus `v2.0.0`, in `~/Downloads/dao-p0/`:

| File | sha256 (verified vs release checksums.txt) |
|------|--------------------------------------------|
| `cw_admin_factory.wasm` | `cc6914b5…f914724` |
| `cw20_base.wasm` (cw-plus 2.0.0) | `4d8e90dd…0d6537d5` |
| `cw20_stake.wasm` | `4ca9ce69…a14b8b7` |
| `dao_dao_core.wasm` | `976ccfb8…b4d1fa42` |
| `dao_proposal_single.wasm` | `149599fb…9c5a2c20` |
| `dao_voting_cw20_staked.wasm` | `5f3bb300…f78935f` |

## 1. Store code (6 txs, permissionless)

```
RELAYER=./target/release/bls-relayer
GRPC=57.129.103.229:9090          # OVH public gRPC
DIR=$HOME/Downloads/dao-p0

for f in cw_admin_factory cw20_base cw20_stake dao_dao_core \
         dao_proposal_single dao_voting_cw20_staked; do
  $RELAYER jc-store-code --layer-grpc $GRPC --wasm $DIR/$f.wasm
done
```

Record each `code_id` (from `store_code` events or `jc-contracts`).
Deployer = default `junoclaw-deployer-v1` key (or `JUNOCLAW_KEY_HEX`).

**Gap probe 1:** whether `store_code_gas` (byte-length formula) is
enough for `dao_proposal_single` (563KB) — largest wasm we've uploaded.

## 2. Instantiate — one tx, max nesting

The stress test: `dao_dao_core` instantiate carries two
`ModuleInstantiateInfo` submessages; the voting module itself spawns
`cw20_base` + `cw20_stake`. Depth: tx → core → voting → token+stake =
4 levels, plus proposal module in parallel.

Inner msgs are **base64-encoded JSON** in `ModuleInstantiateInfo.msg`.

`voting_module_instantiate_info.msg` (base64 of):
```json
{
  "token_info": {
    "new": {
      "code_id": <cw20_base>,
      "label": "JunoClaw P0 token",
      "name": "P0 Token",
      "symbol": "P0T",
      "decimals": 6,
      "initial_balances": [{"address": "<deployer juno1...>", "amount": "1000000"}],
      "staking_code_id": <cw20_stake>
    }
  }
}
```

`proposal_modules_instantiate_info[0].msg` (base64 of):
```json
{
  "threshold": {"threshold_quorum": {
    "threshold": {"majority": {}},
    "quorum": {"percent": "0.20"}}},
  "max_voting_period": {"time": 604800},
  "min_voting_period": null,
  "only_members_execute": true,
  "allow_revoting": false,
  "pre_propose_info": {"anyone_may_propose": {}},
  "close_proposal_on_execution_failure": true
}
```

Core instantiate (the outer `--msg` to `jc-instantiate`):
```json
{
  "name": "JunoClaw P0 DAO",
  "description": "DAO DAO P0 deploy on JunoClaw",
  "automatically_add_cw20s": true,
  "automatically_add_cw721s": true,
  "voting_module_instantiate_info": {
    "admin": {"core_module": {}},
    "code_id": <dao_voting_cw20_staked>,
    "label": "p0 voting",
    "msg": "<base64 voting msg>"
  },
  "proposal_modules_instantiate_info": [{
    "admin": {"core_module": {}},
    "code_id": <dao_proposal_single>,
    "label": "p0 proposal",
    "msg": "<base64 proposal msg>"
  }]
}
```

```
$RELAYER jc-instantiate --layer-grpc $GRPC --code-id <dao_dao_core> \
  --label "JunoClaw P0 DAO" --msg @p0-core-instantiate.json
```

## 3. Gap checklist — what to record

- [ ] store-code accepted all 6 (note any gas/size failure)
- [ ] nested instantiate succeeded through all submsg levels; if it
      failed, at which reply depth
- [ ] contract addresses queryable via `jc-contracts --code-id`
- [ ] `Env.block.height/time` correct inside module instantiation
      (proposal module reads it for voting period)
- [ ] contract→contract queries work (voting module queries staking
      at first vote)
- [ ] DAO DAO event attributes present in archiver `txs`/`tx_addrs`
      projections (events: `wasm-instantiate`, `wasm-propose`, …)
- [ ] a real propose→vote→execute round trip against the new DAO
- [ ] SubDAO path: `cw_admin_factory` instantiate + a child DAO whose
      admin is the parent (second-level nesting)

## 4. Output

Gap list → appended to `DAO-ON-JUNOCLAW.md` and shared with the DAO
DAO team as the P0 answer.

## 5. RESULTS — executed 2026-10-17 against public testnet

**P0 PASSED.** Full propose→vote→execute round trip verified live.

| Step | Result |
|------|--------|
| store-code ×6 | ✅ code_ids 5–10 (factory, cw20_base, cw20_stake, proposal_single, core, voting_cw20_staked) |
| Nested instantiate | ✅ 4 levels in ONE tx: core→voting→(cw20 token + stake) and core→proposal — `handle_success` every depth |
| Contract→contract queries | ✅ `voting_power_at_height` crosses voting→staking→token |
| cw20 send/receive hook | ✅ staked 500k PZERO → voting power 500000 |
| Propose | ✅ proposal #1 open, `total_power=500000` snapshot, `Env.block.height/time` correct |
| Vote → execute | ✅ `{"vote":{"proposal_id":1,"vote":"yes"}}` → status `passed` → `executed` |
| Deterministic addrs | ✅ simulated contract address == real address |

**Deployed DAO:**
- core: `juno1p3pwvxzflj5qew04ydmz0vgjaxxevjlwnyeqxyyd37c8m0tu22tsfc2ylc`
- voting: `juno1h28e7curdp7dna0q50j3ln8j2syx0nkmls44dgy8wkq2mamql49s30k9e3`
- proposal: `juno12eztsvceuv9m5ktk0yjve8ap7sn0g9jtqnkc7jrdmptqdmxeyszqp774ns`
- token: `juno1s4hmd50frkpjvnzv79aqs6xkq4shvjquwqzntk0zqp5vu2zrxjys93vzug`
- staking: `juno13sa8ayd60965d87ehnkpwm52wqwx4gkze2xl2nyj8d8plsxkl2hsq6p3l8`

**Gaps found:**
1. 🐛 `store_code_gas` under-priced VM compile/instrumentation cost
   (~15.6 gas/byte vs the 10/byte formula) — 6 txs burned fees at
   DeliverTx while reporting "broadcast OK" (CheckTx only). Fixed in
   `bls-relayer`: two-phase simulate → re-sign with 20% headroom.
   Wider issue: every `submit_layer_any` caller should use this path.
2. Caller-side notes (not chain gaps): cw20 `symbol` must match
   `[a-zA-Z-]{3,12}`; vote msg variant is `vote`, not `cast_vote`.
3. ✅ **SubDAO via governance PASSED (same day):** `cw_admin_factory`
   instantiated (`juno1wsdg3lwjpn2m2y6k9pwqnzagqkkzp2recr5c2d75a8wmvjj0lvhqsggzcw`),
   then parent-DAO proposal #4 executed a `wasm.instantiate` of a
   child `dao_dao_core` with `admin` = parent — full 2nd-level nesting
   through a governed execute. Child: `juno1g8zarm4u2ezlpeejutflpjxzes9dj8eq8h6hp32ka2x2fjnps70sy8tdwc`
   (healthy: own voting + proposal modules, admin = parent core).
   Correct v2.7.1 msg shapes: `InstantiateInfo.admin` variants are
   `address`/`core_module` (NOT `core_contract`); the proposal module's
   `InstantiateMsg` has NO `dao` field — core injects it.
4. 🐛 **Zombie contract registrations:** a failed nested instantiate
   still registers the contract address — `ContractsByCode` lists it
   but all queries fail (`key "admin" not found`, empty storage).
   Two zombies from failed attempts: `juno1yf4uv…`, `juno1x2yk…`.
   The address allocation isn't rolled back with the rest of the
   execution context → real state-consistency bug in our wasm module.
5. Open: archiver event indexing verification.
