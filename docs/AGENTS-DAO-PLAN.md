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

- [ ] collect operator `juno1` addrs (→ `jclaw1` after cutover)
- [x] store `dao_voting_cw4` + `cw4_group` — **code_ids 12 / 11**,
      on-chain data_hash verified against official checksums:
      `52afbd7f…` (dao-contracts v2.7.1) / `4604a284…` (cw-plus v2.0.0).
      Note: first cw4_group download was a 9-byte 404 page — caught by
      size inspection; always verify artifact size + checksum before
      recording hashes in docs.
- [ ] instantiate Agents DAO (cw4, members = operators+core devs)
- [ ] first ratified proposal: rc12 rollout approval
- [ ] spawn SubDAOs via governance (proven path, runbook §5.3)
- [ ] relay.junoclaw.xyz deploy → move ops coordination to it
- [ ] bech32 `jclaw` cutover window (post-rc12 soak) → redeploy stack
- [ ] fix zombie-contract-registration gap (P0 finding #4)
- [ ] dao.junoclaw.xyz stage-1 read-only console
