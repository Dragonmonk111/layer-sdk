To: Div (InfraSingularity) — reply to validator + infra offer (paste-ready DM)

Hi Div — great to meet you too, and thanks for reaching out.

Quick status so we're aligned: JunoClaw is mid-G1 right now — a
fixed 4-seat closed ceremony set with a 48-hour soak in progress
(closes ~Oct 13, 20:00 UTC). G2 is the public testnet phase, and
external validator onboarding is its first move — so your timing
is good.

Two notes before the call:

Validator seat requirements (we check these for real — our
pre-launch audit just caught one host running 83 s behind NTP):
1. Dedicated LXC/VM, min envelope 2 vCPU / 4 GiB RAM / 20 GiB disk
2. Clock discipline: chrony or systemd-timesyncd holding skew
   <2 s (the chain enforces MAX_FUTURE_DRIFT = 2 s)
3. Stable uptime + a named ops contact we can ping when your seat
   alarms
4. Run our vahana binary from the onboarding doc — one-command
   install, config bundle provided

On the RPC / explorer / indexer offer — genuinely interesting,
but scope it carefully: JunoClaw is not a CometBFT chain. It's a
BLS12-381 threshold-finality chain (sub-2 s finalized blocks,
MAYO2 post-quantum hybrid certificates on every block, gRPC
light-client API). Off-the-shelf Cosmos explorer/indexer stacks
won't drop in — you'd be building against a novel API surface. If
your team is up for that, we'd genuinely value the coverage;
happy to share the API surface + data model ahead of the call so
you can scope the lift.

A quick call this week works on our side — we'll come with the
full G2 technical requirements. It'd help if you bring 2–3 of your
live mainnet seats we can reference-check (Aztec / Starknet /
zkVerify validators are easy to verify).

— [operator]
