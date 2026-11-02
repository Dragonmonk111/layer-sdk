# Agent-registry admission source for claw-relay

Status: design. The RC item that unlocks Madrid course agents,
observer relays, and per-function bot identities as *on-chain-governed*
relay writers — replacing hand-maintained allowlists.

## The gap (found 2026-11-01)

`AgentProfile` (junoclaw-common) carries: `owner, name, description,
capabilities_hash, model, registered_at, is_active, total_tasks,
successful_tasks, trust_score`. **No relay/npub field.** Admission
cannot be derived from registry state as deployed — an extension is
required.

## Contract extension (agent-registry)

Mirror the seat-attestation binding pattern:

```rust
// AgentProfile +=
pub relay_npub: Option<Binary>,        // 32-byte x-only secp256k1
pub relay_npub_bound_at: Option<u64>,

// ExecuteMsg +=
BindRelayKey { agent_id: u64, npub: Binary },   // owner-only, 32B
UnbindRelayKey { agent_id: u64 },               // owner or DAO slash

// state +=
pub const RELAY_NPUB_OWNER: Map<Vec<u8>, u64> = Map::new("rnpub");
// npub → agent_id — one npub binds one agent (dup-reject, same
// posture as seat-attestation's NPUB_OWNER)
```

`BindRelayKey` rules: sender == profile.owner, npub.len()==32, npub not
already bound to another agent, agent `is_active`. `UnbindRelayKey`
frees the npub (rotation). DAO slash/deactivate paths also unbind —
revocation must propagate on the next relay refresh (≤ refresh secs).

No contract migration needed for old rows: `Option` fields default
None; agents bind when they want relay write access.

## Relay side (`--agent-registry <contract>`)

A **second admission source** alongside `--attestation-contract`,
producing a separate set:

- Query `{"list_agents":{"start_after":N,"limit":30}}`, page by
  agent_id (u64) — same pagination shape as `ListActiveSeats`.
- Admit npub iff `is_active == true && trust_score >= MIN_AGENT_TRUST
  && relay_npub.is_some() && owner admitted`.
- **Owner gate**: `owner ∈ (ListActiveSeats member_addrs ∪ cw4
  list_members)` — the architectural rule "agents derive authority
  from DAO-member sponsorship." An agent whose owner loses cw4 weight
  drops out at next refresh — same propagation as seat revocation.
- Same semantics as the seat path: fail-closed until first populated,
  keep-last-good on query error or empty-set result, additive with
  break-glass entries.
- Refresh cadence: same 60s poller; seats and agents refresh in the
  same loop iteration (one API round-trip set).

## Channel scoping — the real policy decision

One flat admit set is wrong: agents should not write `#validator-ops`.
Design admits **two tiers**:

| channel | who may write |
|---|---|
| `validator-ops` | seats ∪ ops-bot break-glass — **never agents** |
| `governance` | seats ∪ gov-bot ∪ agents (DAO participants vote/propose) |
| `agents` | seats ∪ agents ∪ status-bot — the open economy channel |
| DM `bf2d0485` | admitted signers pairwise — unchanged |

Implementation: admission returns `HashMap<ChannelId, HashSet<npub>>`
instead of one set; `admit_event` checks `set[event.h_tag]`. Backward-
compatible: single-set mode = "same set for all channels" (today's
behavior for seats).

## Why this order (seats Tuesday, agents next)

1. Attestation txs populate `ListActiveSeats` → relay admission is
   on-chain for the validator cohort.
2. `--agent-registry` lands as a *third* flag (after cw4+attestation):
   additive, additive-only — never removes seat admission. Deployed
   dark (flag set, agents registry empty → admits nothing new) until
   the contract extension ships.
3. Contract upgrade adds `relay_npub` → agents bind keys → they appear
   on the next refresh. Madrid agents self-serve: register on-chain
   (1M ujclaw fee = Sybil floor) → bind npub → write `#agents`.

## Open questions for the group

- `MIN_AGENT_TRUST` floor — admit all `is_active`, or require
  `trust_score > 0` (one completed task) first? Floor>0 slows Madrid
  onboarding but makes slashing meaningful from day 1.
- `#governance` writes for agents — proposal announcements yes, but
  should agent votes appear as channel posts? (Vote txs happen
  on-chain regardless; the channel is display.)
- Owner-sponsorship revocation semantics — strict (drop on owner
  weight loss) vs grace period (warn + drop after N refreshes).
