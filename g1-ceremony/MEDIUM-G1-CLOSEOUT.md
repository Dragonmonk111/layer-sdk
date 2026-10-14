# JunoClaw G1: our chain halted for 106 minutes — and that's the good news

*G1 close-out, 2026-10-13*

We just closed **G1**: JunoClaw's first closed validator ceremony — a
**48-hour continuous soak** of a 4-seat BFT chain running hybrid
BLS12-381 + MAYO2 (post-quantum) consensus certificates, with
deterministic CosmWasm execution on top of a Commonware simplex
consensus engine.

The raw record: **361,709 finalized blocks** (heights 512,776 →
874,485) across 48.25 hours, **zero divergence** — and **one complete
finalization halt that lasted 106 minutes**, plus a long tail of short
freezes (58 events ≥90 s; 7.5% of the window spent halted, mostly in
and around the incident).

This is the honest record of all of it.

## What G1 was actually testing

G1 wasn't a performance marketing exercise. It was a *ceremony*: a
small, closed validator set generating real threshold keys, running
real consensus, under observation, with the explicit goal of finding
what breaks before external operators join.

The stack under test:

- **Consensus**: Commonware simplex — a pipelined BFT engine
- **Certificates**: hybrid — every finalized block carries a BLS12-381
  aggregate signature AND a MAYO2 (post-quantum signature scheme)
  certificate. Both halves must verify.
- **Execution**: deterministic CosmWasm — same input, same bytes out,
  verified by consensus on the post-block state root
- **Validator set**: 4 logical seats, 3-of-4 quorum

## The two regimes — what the cadence actually looked like

The soak did not run at one speed. It ran in two regimes, and the
split is the most interesting data in the record:

- **Hours 0–25 (fully distributed quorum):** ~3.5 s median block
  cadence — roughly 850–1,200 finalized blocks per hour. This is what
  geographically honest consensus costs: two WAN round-trips plus
  per-vote post-quantum signing.
- **After the incident recovery (hours 26–48):** the cadence collapsed
  to **0.11–0.5 s per block** — over 30,000 blocks in the best hour.
  Once quorum was consistently forming on the lowest-latency seat
  paths, the engine ran near single-host speed *with the hybrid PQ
  certificate still on every block*.

Read that second number again. A post-quantum-hybrid BFT chain doing
~9 finalized blocks a second isn't a devnet fantasy — we watched it
happen for a day. The certificate cost isn't the bottleneck; network
latency between independent operators is. Both numbers are real, both
are published, and per-signer attribution is verifiable from the
certificate bitmaps on the public explorer.

*As this publishes, the same chain is finalizing ~4 blocks per
second — verify it yourself at explorer.junoclaw.xyz.*

## The wedge — what actually happened

At ~17:35 UTC on 2026-10-12, finalization stopped. The chain produced
no new certified blocks for 106 minutes, then recovered and ran
through the end of the window.

Root cause — and this is the part that matters:

1. **A home ISP outage took out two seats at once.** Our 4-seat
   logical set mapped onto only **3 independent failure domains**: two
   seats (index 0 and index 1) lived on the same home host behind the
   same home broadband. When that ISP dropped for ~80 minutes, quorum
   went from 3-of-4 to 2-of-4.

2. **Clock drift extended the outage's tail.** The WSL environment
   hosting those seats was ~83 seconds behind real UTC. Our
   authenticated P2P handshake rejects peers outside a tight synchrony
   window — so even after the ISP came back, the seats couldn't
   re-join until the clock was corrected.

The protocol did exactly what BFT is supposed to do: **it halted
rather than forked.** Zero divergence was observed across the entire
window — no two honest validators ever certified conflicting blocks at
the same height. Safety held; liveness correctly degraded to a halt
when quorum was lost.

## The honest verdict

> **G1 verdict: PASS with one documented wedge (106 min), root-caused
> to a shared failure domain + clock drift — not a consensus weakness.
> Zero divergence. Full recovery. Post-recovery cadence faster than
> pre-incident.**

We are publishing this number because it's the number. A chain that
claims zero halts in week one is either lying or wasn't watched
closely enough.

## What it proved

- **BFT safety holds under real failure.** The failure mode was halt,
  never fork. Certificates (both halves) verified throughout.
- **The architecture is operationally real.** Peers re-authenticated,
  sessions re-formed, heights converged — after fixes, a subsequent
  ~3.5-minute freeze self-recovered with no human touch.
- **Speed is a latency question, not a certificate question.** The
  back half of the soak settled at ~0.1–0.5 s blocks once quorum paths
  were tight. The PQ hybrid certificate was not the constraint.
- **We learned the lesson G2 is built on**: fault tolerance equals the
  weakest domain *correlation*, not the seat count. A 3-of-4 set
  tolerates one *independent* failure domain — and two seats on one
  ISP are not independent.

## Same week, same code: the public surface went live

The ceremony wasn't the only thing that shipped. During the soak
window we deployed the community-owned infrastructure layer, all
open-source, all running now:

- **explorer.junoclaw.xyz** — live blocks, per-block hybrid
  certificate panels (BLS aggregate + MAYO2 + signer bitmaps), JSON API
- **api.junoclaw.xyz** — TLS-terminated public gRPC edge (light-client
  proofs, CDC stream, Cosmos queries)
- **status.junoclaw.xyz** — independent 30-second probe monitor with
  public uptime history
- **cdc-archiver** — a resume-safe full-history indexer writing every
  state change to disk on community hardware, with typed projections
  (accounts, balances, contracts) for address-level search

The dashboards of truth are ours — and they're cloneable. Anyone can
run the same stack.

## What changes for G2

The G2 public testnet inherits a hard rule written directly from this
incident:

> **No two validator seats may share a host or network failure
> domain.** Onboarding requires independent infrastructure, clock
> discipline (<2 s drift), a reachable P2P endpoint, and a named ops
> contact.

G2 targets an expanded set (n=5, then mainnet n=7) with external,
independent operators. Set changes are coordinated atomic ceremonies
rehearsed on devnet — not rolling edits.

## What's still ahead (mainnet gates, no dates)

- Dealer-free DKG for validator key generation
- External review of the MAYO2 integration
- ICS-20 channel + transfer testing
- Governance/treasury multisig handoff
- Full-dress genesis + contract + IBC + PQ rehearsal

We publish mainnet dates only after gates hold. Until then: targets.

## Why we're building this

JunoClaw's thesis: autonomous agents doing real work need an execution
layer where the *evidence* is deterministic. You can't ZK-prove a
model's intent — so we made everything around it verifiable instead:
consensus certificates you can check, state proofs you can verify,
deterministic contract execution, and a task/escrow/truth-market
surface where agent work produces auditable receipts.

G1 was the first honest test of that substrate. It broke in exactly
the way early chains break — operations and topology, not math. And
when the operations held, it ran at ten blocks a second with a
post-quantum certificate on every one.

The chain is still running. G2 opens next.

---

*JunoClaw is built on the Commonware consensus engine, the
Lay3r/LAYER SDK lineage, and runs hybrid BLS12-381 + MAYO2
certificates. Code and ceremony records are open; stats in this post
are from `g1-ceremony/soak-stats.mjs` / `soak-stalls.mjs` over the raw
48.25 h soak log — height 512,776 → 874,485, 361,709 blocks, 58
freezes ≥90 s, median hourly cadence 3.47 s, best sustained hour 0.11
s. Verify the certificates yourself: explorer.junoclaw.xyz.*
