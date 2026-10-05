# Agent Stack on the Sovereign Chain — Living Notes

Started 2026-10-05. **This is the working notebook for the agent layer.** Update
the Progress Log and Open Questions as work lands; never delete history.
Companion: `docs/AGENT_RELAY_PLAN.md` (replacement for buzz.junoclaw.xyz).

> Thesis: the chain is deliberately thin (instant-finality, hybrid PQ certs,
> deterministic Wasm). The product is **verifiable agents and robots**: agents
> hold money and keys, do work, and *prove* it; the chain settles; staked
> verifiers adjudicate what can't be proven by math alone.

---

## 1. Layers (bottom to top)

| Layer | What | Contracts / components |
|---|---|---|
| L0 Substrate | junoclaw-chain (Commonware simplex, BLS + MAYO2 hybrid certs, CosmWasm VM with BN254 host fns, denom `ujclaw`, **no staking module**, fees to proposer) | `slay3rd`, `layer-app`, `tx-sender` |
| L1 Identity | who is an agent; keys; credentials; sealed signing | `agent-registry`, `jclaw-credential`, `tee-attestation-verifier`, sealed signer (WAVS/TEE) |
| L2 Work | post, match, escrow, complete tasks with machine-checkable constraints | `task-ledger`, `escrow`, `marketplace`, `skill-registry` |
| L3 Verification | decide whether work/claims are true | `zk-verifier` (Groth16/BN254), `jolt-cw-verifier`, `tee-attestation-verifier`, **`truth-market`** (staked operators, verdict, slash), `coordination-settler` |
| L4 Memory / provenance | immutable rationale, shared knowledge | `moultbook`/`moultbook-v0`, `knowledge-moults`, `merkle-verifier` |
| L5 Governance | agents and token holders steer params | `agent-company` (agent DAO), `junoclaw-dao` (lock-to-vote), `airdrop-claim`, `builder-grant` |
| L6 Robotics safety | physical-world gating | `safety-envelope`, `circuit-breaker`, `machine-rwa`, `emergency-compute-escrow` |
| L7 Coordination (off-chain) | discovery, bidding, pre-consensus verdict drafts, telemetry | Buzz today -> **claw-relay** (see relay plan) |
| L8 Product | UI: robots + agents front and centre | frontend `BuzzPanel`, MCP server (28 tools), x402 shim, ROS2 bridge, prover-daemon, J-Lens probes |

## 2. End-to-end flow (the thing that must work, in order)

1. **Onboard.** Agent/robot derives a hybrid account (secp256k1 + MAYO) ->
   `agent-registry.Register` (fee) -> optional `jclaw-credential` (PQ cred) ->
   optional sealed-signer attestation via `tee-attestation-verifier`.
2. **Discover.** Tasks/capabilities appear on the coordination layer (relay
   channels) and on-chain (`task-ledger` events, `skill-registry`).
3. **Post work.** Requester `task-ledger.SubmitTask` with pre/post **hooks**
   (`TimeAfter`, `BlockHeightAtLeast`, `EscrowObligationConfirmed`) and
   `escrow.Authorize` (funds locked, payee fixed).
4. **Coordinate (off-chain).** Bids, clarifications, draft verdicts gossip on
   the relay. Nothing here is trusted; it only has to be *checkpointed*.
5. **Execute + evidence.** Agent/robot does the work; evidence = ZK proof
   (`zk-verifier`/`jolt`), TEE attestation, content hash in `moultbook`.
6. **Verify.** `truth-market`: requester/relayer `PayVerificationFee` ->
   registered operators `SubmitVerdict` (open-weight J-Lens, green/yellow/red)
   -> relayer `FinalizeEpoch` -> reward matching majority, slash divergent.
   Missing/invalid proof => auto-Red.
7. **Settle.** `task-ledger.CompleteTask` evaluates post-hooks (escrow
   `Confirmed`, timelocks) -> `escrow.Confirm` releases funds ->
   `agent-registry` trust score up / slash on dispute.
8. **Remember.** Rationale + verdict posted to `moultbook`; relay channel
   checkpoint anchored on chain.
9. **Govern.** `agent-company` / `junoclaw-dao` proposals tune fees,
   thresholds, operator sets, constitutional upgrades (67% supermajority).

Synchronicity rule: **chain = source of truth and money; relay = fast,
verifiable gossip; both bound by the same account keys.** An off-chain message
is only ever advisory until its hash/commitment is on chain.

## 3. Contract matrix — what was where

Sources: `junoclaw/deploy/deployed-testnet.json`, `deployed-mainnet.json`,
`docs/TIER15_TESTNET_RUN.md`, `docs/OPEN_ENDS.md`, `articles/WHAT_IS_JUNOCLAW_2026_09_11.md`.
`deploy/deployed.json` (full history incl. truth-market/marketplace/machine-rwa
addresses) is gitignored and was **not** read.

| Contract | uni-7 (Juno testnet) | juno-1 (Juno mainnet) | Sovereign devnet | Needed for e2e |
|---|---|---|---|---|
| agent-registry | code 69, `juno15683x0sa...` | not deployed (planned in MAINNET_DEPLOY_PLAN) | **code 5** `juno1ej92ut6...` | yes |
| task-ledger | v6 code 70 frozen (no wasmd admin); **Tier1.5 code 75** `juno1cp88zj8...` | no | **code 6** `juno1kcaqdc0...` | yes |
| escrow | code 71 `juno17vrh77v...` | no | **code 7** `juno1lhthdtw...` | yes |
| agent-company | v3 code 63 (Mar), v6.1 code 72 `juno1lymtnjr...` (later "v4" per Sept article) | no | **code 9** `juno1lw3677t...` | yes |
| truth-market | live on uni-7 (Aug); address in gitignored deployed.json | no | **code 10** `juno1mv5nxlz...` | yes |
| marketplace | live on uni-7 (Aug) | no | **code 12** `juno1uj79fwh...` | yes |
| skill-registry | code 82 `juno1pug0zu6...` | **code 5145** `juno1wp5fpcx...` | **code 11** `juno1aylyt4c...` | yes |
| moultbook (v1) | code 80 `juno1nm0mu2u...` | **code 5148** `juno1r59ulw6...` | TODO | optional |
| moultbook-v0 | live (feepay tests) | `juno18xn4cfp...z6` (A13 heartbeat, from memory) | **code 8** `juno1a6szypt...` | yes (provenance) |
| zk-verifier | code 78 pure-wasm (`juno19jk0dnv...`); no BN254 precompile on uni-7 | **code 5146** `juno1qd9qagg...` (v30 BN254) | **proven** (Groth16 77,590 gas, Sept) | yes |
| jolt-cw-verifier | code 113 `juno1h7z2pmm...` (103 deprecated) | no | **proven** (68KB proof, 10.87M gas) | optional |
| jclaw-credential | code 79 `juno1z2w067p...` | **code 5147** `juno1dgmakav...` | TODO | optional |
| coordination-settler | code 86 `juno16gp6mm7...` | no | TODO | optional |
| tee-attestation-verifier | proof-of-concept (SGX) | no | TODO | optional |
| emergency-compute-escrow / machine-rwa | live on uni-7 (Aug) | no | TODO | later (robots) |
| ibc-task-host / cw-ics20-transfer | codes 108 / 107 | no | **skip** (no IBC yet) | no |
| junoswap-factory/pair | codes 61 / 60 / 74 | no | skip | no |
| airdrop-claim | n/a | n/a | **proven** (Sept 19) | G2 |
| junoclaw-dao (lock-to-vote) | n/a | n/a | **code 4** `juno1tsgw7me...hfs2` (Oct 4) | governance |
| builder-grant | code 73 | no | skip | no |
| safety-envelope / circuit-breaker / merkle-verifier | built | no | TODO (robotics) | later |

Facts worth remembering:
- Mainnet currently holds only 4 JunoClaw contracts (skill-registry, zk-verifier,
  jclaw-credential, moultbook). The sovereign chain will be the **first full
  deployment** of the stack.
- Every instantiate **must set the wasmd migrate admin** (6th arg). v6
  task-ledger/escrow/agent-company were frozen on uni-7 for omitting it.
- uni-7 block time drifts behind wall clock (~15 min observed). Compare
  `TimeAfter` against chain time, never wall clock.

## 4. Dependency-ordered deploy for the sovereign chain

Instantiate-time wiring from the real `InstantiateMsg`s:

1. `agent-registry` {admin, max_agents, registration_fee_ujuno, denom, registry:None}
2. `task-ledger` {admin, agent_registry, operators, agent_company:None, registry:None}
3. `escrow` {admin, task_ledger, timeout_blocks, denom, registry:None}
4. `agent-registry.UpdateRegistry {task_ledger, escrow}` and
   `task-ledger.UpdateRegistry` (close the circular pointer graph)
5. `zk-verifier` {admin}
6. `moultbook-v0` {admin, max_size_bytes, max_refs, max_content_type_len,
   max_group_size, zk_verifier, agent_registry, ...}
7. `agent-company` {name, escrow_contract, agent_registry, task_ledger,
   zk_verifier, moultbook, members(sum=10000), denom, ...}; then back-wire
   `task-ledger.agent_company`
8. `truth-market` {min_stake, slash_percent, reward_percent, denom,
   unstake_cooldown_secs, min_operators, reward_mode, verification_fee}
9. `skill-registry` {admin, denom, registration_fee}
10. `marketplace` {admin, truth_market, task_ledger, skill_registry, denom, cancel_window_secs}

Chain-specific overrides: `denom = "ujclaw"` everywhere (defaults are
`ujunox`/`ujuno`); no IBC contracts; JSON numbers for `u64`, **strings** for
`Uint128` in cosmwasm-std 1.x (the DAO used raw `u128` -> JSON numbers; check
each msg); unit enum variants are bare strings; use `tx-sender --msg-file`.

## 5. Tooling on the sovereign chain

- `tx-sender` (`tools/tx-sender`): store-code, instantiate, execute (now with
  `--amount/--denom`), query, bank send, get-tx, bench. gRPC `127.0.0.1:9090`.
- Deployer: `juno1dz875zg8p78anpjv3f0qt4gu5a3awpjfhtw992`
  (SHA256("junoclaw-deployer-v1")), also `wasm.gov_account`.
- `TX_SENDER_KEY_SEED=<label>` (devnet only) signs as SHA256(<label>) instead of
  the deployer, so test actors can use the same tool (used by
  `scripts/agent-e2e.ps1`).
- Wasm build: `junoclaw/contracts/.cargo/config.toml` already sets
  `-bulk-memory,-reference-types`; raw `cargo build --release --target
  wasm32-unknown-unknown --lib -p <pkg>` works (DAO proved it).

## 5.1 Devnet deployment and e2e results (2026-10-05)

Deployed by `scripts/deploy-agent-stack.ps1` in the section 4 order with the
registry pointer graph closed. Deploy record: `snapshot/agent-stack-deployed.json`
(untracked, devnet-local).

| Contract | code | address |
|---|---|---|
| agent-registry | 5 | `juno1ej92ut6dkwc8x6yyyqdjfkxwrp9rs8aw59nv6usvzzuh6phs6ufqfkxurh` |
| task-ledger | 6 | `juno1kcaqdc0ngvlj8glf4xlr50nd7jvfdh2kqcj3ep38qfue9zas49usw4nsqd` |
| escrow | 7 | `juno1lhthdtwdxs6mh7flw2mgqpmzrg480pqgunxpzjm9w0u57ttrz3cqzgq9qs` |
| moultbook-v0 | 8 | `juno1a6szypthyj97f7ly930euqlgzzqqq07j7mlpamuh098uqx0htcdqexhsqa` |
| agent-company | 9 | `juno1lw3677tmx37e600k8qyny6tyy2337y8lxn4ajqym6k7eft8mgaeqy0rf4d` |
| truth-market | 10 | `juno1mv5nxlzp63v9mhrlu9z3q3lymnqkx3rg6h2r0qd4aq8x379a66mqvmqaqx` |
| skill-registry | 11 | `juno1aylyt4ctnnac5e455lrpre5m5407hjq70dyrgfaw6hn6wn84022se090ru` |
| marketplace | 12 | `juno1uj79fwhvt658st6u8ep6jjddtq40ctyemrrxc8jyf27ryela4smsvzjp7x` |

`scripts/agent-e2e.ps1` drives the section 2 flow with six deterministic actors
(`owner`, `req`, `v1`-`v3`, `atk`). Phases run in order, or one at a time with
`-Phase setup|agent|custody|escrow|probe|prov|summary`. Result on the live
devnet: **71 assertions, 0 failures, 2 vulnerabilities reproduced.**

| Phase | Asserts | Covered |
|---|---|---|
| setup | 8 | six actors funded (30,000,000 ujclaw each); `task-ledger.agent_company` and `truth-market.min_operators = 3` wired |
| agent | 6 | registration fee enforced (1,000,000), agent id 1; skill published; service listed at 500,000 |
| custody | 28 | `BlockHeightAtLeast` pre-hook blocks early completion, then allows it; hire escrows 500,000; agent `total_tasks` and `trust_score` +1; 3 operators stake 1,000,000 each; verdict guards (0 verdicts, duplicate verdict, 2 of 3 `min_operators`, non-admin finalize) all rejected; finalize with 2 matching / 1 diverging: slash 100,000 (10%), rewards 150,000 (5% of the pool); `release_on_verdict` pays the agent owner 500,000; double release rejected |
| escrow | 11 | task with `AgentTrustAtLeast` + `EscrowObligationConfirmed` hooks stays blocked while the obligation is Pending; requester pays 250,000 off-contract and confirms with the tx hash; attacker `confirm` rejected; completion then passes and trust +1 |
| probe | 11 | reproduction sequences for F1 and F2 (below) |
| prov | 7 | moultbook rejects an unknown ref, accepts a receipt citing a real entry id, `list_by_ref` finds it, credit score 100 |

### Findings (both reproduced on the devnet)

Both are in the current `junoclaw/contracts` source. Neither contract is on
juno-1 mainnet (only skill-registry, zk-verifier, jclaw-credential, moultbook are).

**F1 - marketplace verdict is not bound to the hire.** `ReleaseOnVerdict
{ hire_id, batch_height }` accepts any finalized truth-market epoch the caller
names. An unrelated all-red epoch (`messages_hash = sha256:UNRELATED`) moved the
hire of an already-completed task to `slashed` and refunded the requester
500,000 (net +400,000 after the 100,000 tx fee). By the same lack of binding a
green epoch could release a hire for unverified work (inferred, not tested).
Proposed fix: record the epoch (`batch_height` / `messages_hash`) on the hire
when verification is requested, require `epoch.messages_hash == hire.output_hash`
on release, and restrict who may trigger it.

**F2 - escrow `task_id` squatting spoofs the payment hook.** `Authorize
{ task_id, payee, amount }` is unauthenticated and first-come-first-served per
`task_id`. The attacker authorized task C with itself as payee for 1 ujclaw, the
real payer then got `Task already has an obligation`, the attacker confirmed its
own obligation, and `EscrowObligationConfirmed` (status check only) let
`CompleteTask` pass although the owner was never paid. `escrow.total_confirmed`
ended at 250,001 (250,000 genuine + 1). Proposed fix: only the task's requester
(task-ledger lookup) may authorize, and the hook must pin `payee` and a minimum
`amount`.

### Observed behaviour worth keeping

- Contract errors come back in `raw_log` as `Contract Error: ...`; hook failures
  name the hook: `pre_hook: hook[i]: <Kind>: ...`.
- Heights advance 3-4 blocks per sequential tx cycle (broadcast + `get-tx --wait`).
  Per-tx fee in `tx-sender` is 100,000 ujclaw.
- `task-ledger.get_tasks_by_agent` returns newest first. `moultbook.list_by_author`
  returns id (hash) order, not chronological: diff before/after to find a new id.
- `moultbook-v0.Post` `refs` must be existing entry ids (cite-only-real-entries).
  Credit score = (active * 60 + attested * 40) / total, capped at 100.
- `truth-market`: slashed stake goes into the reward pool. After two epochs the
  pool is 2,802,502 = 3,000,000 + 100,000 - 150,000 - 147,498 (second epoch:
  3 x 49,166 = floor(5% of 2,950,000) split equally). Only the admin (deployer)
  can finalize today.
- Final stats: task-ledger 4/4 completed; truth-market staked 2,900,000;
  marketplace volume 1,000,000 (released 500,000, slashed 500,000); moultbook 2
  entries; skill-registry 1.

## 6. Open questions

- Does `agent-company` / `truth-market` call anything the sovereign VM lacks
  (staking/distribution queries, `cosmwasm_2_x` caps)? Find out by deploying.
  **Answered 2026-10-05:** all eight contracts instantiate and execute on the
  sovereign VM; seven are exercised end to end. `agent-company` is only
  instantiated and queried so far (proposal / verification flows untested).
- Who is the `relayer` for `truth-market.FinalizeEpoch` on the sovereign
  chain? Today a trusted admin/relayer. Target: BLS-threshold
  `coordination-settler` or relay-registry-attested quorum.
- `truth-market` slashing economics now that `ujclaw` has no staking module:
  stake is contract-held `ujclaw`; confirm unstake cooldown and slash sink.
  **Partly answered 2026-10-05:** slash sink = reward pool (see 5.1); unstake
  cooldown still untested.
- Should `marketplace` + `truth-market` get a Akash-style reverse-auction
  "verification request" primitive, or stay as-is and put bidding on the relay?
  (See relay plan section 5.)
- F1 / F2 fixes (see 5.1): bind marketplace hires to a specific epoch / output
  hash, and restrict `escrow.Authorize` to the task requester. Both contracts
  live in `junoclaw/contracts` (separate repo): decide fix-in-place + migrate vs.
  redeploy on the devnet, then re-run the probe phase expecting `OK`.

## 7. Progress log (append only)

- 2026-10-05: Inventory done from repo docs (this file). Relay plan written.
  Core agent contracts building for wasm32. Next: deploy in section 4 order
  and run flow section 2 steps 1-3,6-7 on devnet.
- 2026-10-05 (evening): Deployed the 8-contract agent stack on the devnet (codes
  5-12, registry graph wired) with `scripts/deploy-agent-stack.ps1`.
  `scripts/agent-e2e.ps1` ran every phase green: 71 assertions, 0 failures.
  The probes reproduced F1 (marketplace verdict not bound to the hire) and F2
  (escrow `task_id` squatting spoofs `EscrowObligationConfirmed`), see 5.1.
  `tx-sender` gained `TX_SENDER_KEY_SEED` (devnet-only multi-actor signing).
  Next: fix F1/F2 in `junoclaw/contracts` and re-run probes; then claw-relay R0.
