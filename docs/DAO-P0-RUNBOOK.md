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
