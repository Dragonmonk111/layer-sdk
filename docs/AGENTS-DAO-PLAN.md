# JunoClaw Agents DAO — Structure, Voting Model, and Ops-Chat Fusion

Status: design — post-P0 (DAO DAO v2.7.1 suite proven live, incl. SubDAO
nesting via governance). Supersedes: nothing; feeds P1 in
`DAO-ON-JUNOCLAW.md`.

## 1. Voting model — cw4 membership over cw20-staked

**Decision: `dao_voting_cw4` + `cw4-group`.** Ops governance wants
"these N keys are members," not a market.

| | cw4 (membership) ✅ | cw20-staked |
|---|---|---|
| Power | Explicit member list, arbitrary weights | Token-derived |
| Change member | Proposal updates group | Buy/acquire/stake |
| Skin-in-game | Social/operational | Financial — meaningless before a real token exists |
| Pre-mainnet fit | **Correct** — ops DAOs aren't markets | Theater: we'd mint a test token then pretend it's a vote |
| Complexity | One contract | Token + staking + distribution ops |
| Bot seats | Trivial — add bot pubkey as member | Bot must hold/stake — extra plumbing |

cw4 also handles **weighted seats** (weight per member) if we ever want
grading to affect vote weight — for now keep weight=1, reputation via
contribution records, not consensus.

**Future: credibility-weighted agent ballots.** `weight` per cw4 member
can become `f(agent-registry record)` — verified task completions,
truth-market verdicts, uptime attestations, tenure. Constraint: the
weight table is a *ratified* artifact (members approve a weights
proposal derived from on-chain records) — never self-asserted by the
agent or its contract. Candidate shape: human operator seats stay
weight 1; agent seats earn 0→N from proven work, capped and revocable.
Parked, not scheduled — depends on the agent stack producing enough
verifiable history to grade against.

## 2. SubDAO structure + treasury seats

Proven path: parent DAO proposal → `wasm.instantiate` child
`dao_dao_core` with `admin = parent`. Live-verified on testnet
(P0 SubDAO `juno1g8za…`).

```
JunoClaw Agents DAO (root, cw4: all operators + core devs)
├── Validator Ops SubDAO  — all ACTIVE validators; seat changes,
│                           upgrades, emergency pause proposals
├── Treasury SubDAO       — starts {Taj, Ravi} 2-of-2; expands by
│                           contribution grading (below)
└── Infra SubDAO          — public RPC/explorer/faucet operators
```

**Treasury seats:** not all validators get one. Start minimal
({Taj, Ravi} — first two operators with sustained infra contribution),
expand to 5-of-7 as members graduate. **Contribution grading** must be
bias-resistant — score only verifiable artifacts:

- signing uptime (on-chain: cert bitmaps per height — objective)
- ceremony participation (on-chain: DKG transcript presence)
- merged PRs (git log — reviewable, but gate weight behind two
  existing-member acks to prevent self-grading)
- infra receipts (off-chain, posted to the ops channel, ratified by
  proposal)

Rule: a seat change is ALWAYS a proposal, never an admin action —
grading feeds the proposal, humans ratify it.

## 3. bech32 prefix — `juno1…` → `jclaw1…` (before G5)

Currently `LAYER_BECH32 = "juno"` — a display + derivation constant.
The underlying 20-byte address hash is unchanged; the string changes.

**Blast radius:** contract state stores bech32 *strings* (admins,
members), so every live contract references old-form addrs → the
DAO stack must be redeployed after cutover (cheap — P0 runbook exists).
Account/bank state is keyed on raw bytes — unaffected. No consensus or
DKG impact — **this is an app restart, not a reshare.**

**Timing:** NOT in rc12 (keep the consensus-hardening window clean).
Dedicated **bech32 cutover window right after rc12 soak**, before more
contract state accumulates and well before G5 genesis design:

1. bump `LAYER_BECH32` → `jclaw`, rebuild node + tooling (rc13)
2. rolling restart (same binaries ops already run, new const)
3. redeploy DAO stack under new addrs (rc13 store-codes, fresh
   Agents DAO instantiate with cw4)
4. update keygen default + operator checklist
5. never reuse `juno` — it collides socially with Juno Network anyway

## 4. Governance-channel fusion — chat IS the DAO surface

The 2022 Jake idea: team chat and DAO coordination in one substrate.
Now feasible because we have both halves: signed-event relay semantics
(buzz-style) and a live DAO.

**Design — "deliberation is a channel":**

- ONE relay (`relay.junoclaw.xyz`) hosts everything: ops chat,
  governance channel, agent channels. **No second substrate** —
  Telegram stays for public comms only, all operator coordination
  moves to the relay.
- Every DAO gets a channel (NIP-29 `h`-tag scoped, ML-KEM-768
  encrypted per channel — per `AGENT_RELAY_PLAN.md`).
- **Proposal↔thread binding:** creating a proposal auto-emits a signed
  channel event `{kind: PROPOSAL, dao, proposal_id, title}`; the
  proposal's `description` carries `channel+event_id`. Deliberation =
  replies in that thread; votes = on-chain txs referencing the same id.
- **Bot members:** deterministic agents (uptime monitor, treasury
  accountant, truth-market resolver, robot-ops coordinator) get
  `jclaw1` accounts AND cw4 membership. Their votes are deterministic
  policy agents — rule + input + vote all auditable: the rule is a
  public artifact, the vote is on-chain, the rationale posts to the
  thread. This is the "verifiable deterministic agents suite" path.
- Novelty vs. stock DAO DAO: the relay is the deliberation layer and
  the identity bridge — operator chat key, DAO vote key, and bot key
  are one signature namespace, so a channel message and a ballot are
  the same kind of signed artifact.

**Build order:** (a) relay deploy + ops channel → (b) proposal-watch
bot that posts proposal events + results → (c) Agents DAO (cw4) →
(d) wire proposal↔thread convention → (e) first bot member.

## 5. dao.junoclaw.xyz — UI and signing without Keplr

No Keplr → we invent the signing UX. Options in increasing ambition:

| Stage | Signing | Notes |
|---|---|---|
| 1. Read-only console | none | proposals, members, votes, treasury, mesh health — pure gRPC/archiver queries; serves public transparency too |
| 2. CLI-relay signing | operator's box | UI emits unsigned tx JSON → operator runs `jc-sign` on their validator/desktop → pastes signature → UI broadcasts. **Zero key exposure; validators already live by key custody** |
| 3. Browser session key | WebCrypto | key imported once, held in memory only, per-session — fine for testnet ops, flagged as such |
| 4. QR/deep-link pair | desktop CLI | phone/desk scan, payload relayed to signer — pairs with buzz pairing infra |

Custom features no generic DAO UI has: PQ-cert badges per proposal-era,
signer-bitmap health, "this vote ran on 5-seat hybrid BLS+MAYO
finality" provenance line, mesh status inline.

## 5.5 Identity model — one identity, separated key material

The "chat key = vote key" idea is refined as **unified identity, not
unified key material**:

- **Cold `juno1` key** — governance identity (cw4 member). Votes,
  treasury. Rarely used, kept offline/operator-controlled. Eligible
  for PQ-hybrid signing later because JunoClaw owns its tx format and
  already runs MAYO2 certs on every block.
- **Hot `npub`** — relay/chat identity (secp256k1 Nostr). Lives in
  clients and agent hosts, signs constantly, rotatable. A leaked nsec
  impersonates chat but cannot vote.
- **On-chain attestation binds them** — the cold key signs
  "hot npub X speaks for me until T" as a chain artifact (or cw4-held
  record). The relay allowlist admits the hot key; governance audit
  resolves identity through the binding. NIP-26 precedent.
- **Bot seats use the same attestation** — a deterministic policy
  agent's rule is the attestation payload; its npub and (optional)
  juno1 member key are bound the same way.

PQ posture: **auth isn't where PQ urgency lives** — recorded sigs
can't be retro-forged, but recorded ciphertext can (harvest-now-
decrypt-later). Priorities: (1) ML-KEM-768 channel encryption wrapped
inside normal Nostr events (no wire fork — NIP-01 hard-codes 64-byte
Schnorr sigs, so the wire auth *cannot* be PQ anyway), (2) PQ-signed
attestations/bindings on-chain where hybrid is native, (3) hot chat
keys stay secp256k1 — they're rotatable, that's their defense.

**PQ-ready from day one (cheap, no protocol work):**
- The hot↔cold attestation record stores a **MAYO pubkey commitment**
  alongside the juno1 address. When hybrid account signatures land
  (pre-mainnet roadmap item — JunoClaw owns its tx format and already
  runs MAYO2 certs on every block), migration is a binding update,
  not an identity reset.
- Every **ratified** DAO proposal gets a MAYO-signed endorsement
  artifact posted into its `#governance` relay thread — off-chain PQ
  signature over the proposal hash, verifiable with existing tooling.
  Dogfoods our PQ stack on real governance today.

**`cw4_group` ↔ `pubkey_allowlist` sync:** the DAO membership IS the
relay ACL. A watcher agent translates cw4 member changes (after
ratified proposals) into `pubkey_allowlist` inserts/removals —
governance controls the ops channel through the same proposal flow
as everything else.

**Onboarding auto-admission (no manual key asks, ever):** a new
validator's identity bundle is generated during node setup as part of
the operator pack — governance pubkey (secp256k1 → juno1) + hot npub
ship inside `share-request.json` alongside the existing Ed25519/MAYO
material. The DAO proposal that admits their validator seat ALSO
carries `cw4_group` add-member + the npub attestation: **one
ratification = seat + vote + relay access**. The watcher agent then
syncs `pubkey_allowlist` automatically — no DB edits, no re-asking.

## 6. Action list

- [ ] collect operator `juno1` addrs — **3/5 re-verified (2026-10-19)**
      (→ `jclaw1` after cutover). Canonical derivation (the ONLY
      correct path, per `tools/tx-sender derive-address` and the
      chain's own signature→address resolution):
      **`ripemd160(sha256(compressed_33B_pubkey))` → bech32 `juno`**.
      ⚠️ **Derivation incident 2026-10-19:** the first staging round
      used `ripemd160(pubkey)` — sha256 missing — producing five
      addresses no key can sign for. The DAO instantiated on that
      member list is unusable and must be re-instantiated; ~5M ujclaw
      dripped to the wrong addresses is unrecoverable. Discovered when
      the chain resolved home-vb's vote signature to its canonical
      address (`juno1wjtmj0…`), not the staged one. Verified canonical
      addresses (derived from on-box PEM pubkeys, byte-exact):
      | seat | canonical governance addr | status |
      |---|---|---|
      | home-vb | `juno1wjtmj0fekn4fkdpf8ttgctr537txvarae225a0` | pubkey verified from `/opt/g1-node/keys/dao-key.pem`; funded 1M ujclaw |
      | ovh-uk1 | `juno134zsp2umr4vkhkvrn35uy3fv4n5uss233vnraz` | pubkey verified from `/opt/dao-keys/dao-key.pem`; funded 1M ujclaw |
      | posthuman | `juno1yzte2lj9mtn2jlquj28apg82aedrxwywjt0k0m` | derived from operator-supplied pubkey; funded 1M ujclaw |
      | anatoly-hetzner | TBD — awaiting pubkey re-submission | — |
      | vahana | TBD — awaiting pubkey re-submission | — |
      Wrong staged addrs (do NOT use): `juno1p3suqq…` `juno1e9c5kq…`
      `juno1sxd97a…` `juno19e5p4h…` `juno1euyhdn2…`.
      Operator re-submission path: run `tx-sender derive-address
      --pubkey <33-byte compressed pubkey hex>` on-box, or send the
      compressed pubkey — never the private key.
      Old npub bindings in `/opt/buzz/state/members.json` are keyed by
      the wrong addrs and need rebinding to canonical addrs at
      re-instantiation.
      Next: instantiate per `DAO-P0-RUNBOOK.md`. Relay allowlist is
      per-KEY not per-seat — a second operator on a shared seat (e.g.
      Luuk) may allowlist their own npub with no DAO change.
      `allowlist-add.sh` now verifies the bech32 checksum before
      decoding — a mistyped npub fails loudly instead of silently
      allowlisting a garbage key. (vahana's original npub had a
      generator bug — correct 32-byte key, wrong checksum chars;
      the corrected npub decodes to the identical allowlisted hex,
      confirmed.) npub audit of all keys collected so far: anatoly
      checksum-verified, vahana re-verified via corrected npub,
      status-bot + relay identity auth-verified live — no other
      seat used the bad generator.
- [x] store `dao_voting_cw4` + `cw4_group` — **code_ids 12 / 11**,
      on-chain data_hash verified against official checksums:
      `52afbd7f…` (dao-contracts v2.7.1) / `4604a284…` (cw-plus v2.0.0).
      Note: first cw4_group download was a 9-byte 404 page — caught by
      size inspection; always verify artifact size + checksum before
      recording hashes in docs.
- [x] re-instantiate Agents DAO with canonical member addrs —
      **DONE 2026-10-20** (tx `F59D393B…` → DeliverTx code 0,
      `juno122s3kcms…` verified via `dump_state`):
      - core `dao_dao_core` (code 8):
        `juno122s3kcms43lgqu6p3h4tezcnwmpj5kslleku5jnz2a2jvat8mh0qkrqv3u`
      - voting `dao_voting_cw4` (code 12):
        `juno1qevagefp6dlxrl8agkas3thtzxdctw7w6grgqkuv6yvnepls9hhsg74d3l`
      - proposal `dao_proposal_single` (code 7):
        `juno1pt8gwn9ax4pvvlwm0s6nmxpgfvclfr9fmzr54620mqyclqa8k68q45x5wv`
      - group `cw4_group` (code 11, watcher target):
        `juno1u0l6ddahjn88v4usdrk33vk4k53qsjmmun45n9ssvfl5sg5xnj3qul37pj`
      `list_members` verified on-chain: all 5 CANONICAL operator addrs
      (anatoly `juno1p3suqq…`, vahana `juno1xqyqsz…`, home-vb
      `juno1wjtmj0…`, ovh `juno134zsp2…`, posthuman `juno1yzte2l…`),
      weight 1 each. Config verified: majority threshold, quorum 0.60,
      604800s voting period, only_members_execute. Anatoly's staged
      addr was canonical all along (buggy path coincided); vahana
      supplied a fresh key (`02d0f338…` → `juno1xqyqsz…`).
      `/opt/buzz/state/members.json` rebound to canonical addrs +
      `group_addr` updated; dao.junoclaw.xyz repointed.
      Proposal #1 (rc12 ratification) reopened, proposer treasury,
      start_height 2576326 — OVH seat YES committed.
      First instantiation (below) remains as the dead on-chain
      instance — do not reuse.
- [x] instantiate Agents DAO — **DEAD INSTANCE** (tx `B8FAA0C8…`,
      2026-10-19; 4/5 member addrs wrongly derived — superseded by
      `juno122s3kcms…` above):
      - core `dao_dao_core`:
        `juno1slsqfk4dgapg2tfm0t7uz9lr9vt9kl05ng7v46atdg6tkefz0uuqprpt6x`
      - voting `dao_voting_cw4`:
        `juno1k2l0rqedqstkv8p3el9sld3n3nwxpa0cmt5wg4r49046sx432sqqf0yuqk`
      - proposal `dao_proposal_single`:
        `juno1alfx20vzr9yv8hm37wga3t6kqe03r7fzzsvfhpylxjdfe8v5w34qx6hffa`
      - group `cw4_group` (watcher target):
        `juno185ry94ex3esnxtc8ffv2azugtyrh97u7642ktfn4qm07z660w2cspshh9d`
      `list_members` verified on-chain: all 5 operator addrs, weight 1.
      Relay watcher live (group_addr written; quoting bug in the
      argv pass fixed). Note: existing allowlist rows pre-date the
      watcher and carry manual tags — the DAO removal path only
      removes `dao-member`-tagged rows, so member removals until
      then need a manual allowlist pass (documented RELAY-OPS).
      Tooling gaps found: `jc-instantiate` broadcasts with fixed
      LAYER_GAS (no two-phase simulate — first attempt failed
      silently at DeliverTx on a msg-schema mismatch; schema fix
      documented here) and its ContractsByCode wait-loop returns
      on first non-empty result (can't distinguish new contracts
      when the code_id already has contracts). Fix both in the next
      bls-relayer pass. dao_voting_cw4 v2.71 instantiate msg shape:
      `{"group_contract":{"new":{"cw4_group_code_id":N,
      "cw4_group_salt":null,"initial_members":[...]}}}`.
- [~] first ratified proposal: rc12 rollout approval — proposal #1 OPEN on new module (quorum 0.60); votes needed: 3/5 for quorum + majority
- [ ] spawn SubDAOs via governance (proven path, runbook §5.3)
- [ ] relay.junoclaw.xyz deploy → move ops coordination to it
- [ ] bech32 `jclaw` cutover window (post-rc12 soak) → redeploy stack
- [ ] fix zombie-contract-registration gap (P0 finding #4)
- [ ] dao.junoclaw.xyz stage-1 read-only console
