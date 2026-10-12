To: Anatoly — G2 validator seat (paste-ready DM)

Hey Anatoly — status, and the answer to your G2 question.

G1 is a fixed 4-seat closed ceremony set — no seat changes mid-soak,
which is why your Proxmox box sat G1 out. Nothing personal; G1 is a
controlled keygen/soak environment.

G2 is where you come in. We close G1 right after the soak wraps
(48 h mark ~2026-10-13 20:00 UTC), then open G2 — the public
testnet phase — and validator recruitment is the first move. You're
on the shortlist to be one of the first external validators.

What we'll need from you:
1. A dedicated LXC/VM on the Proxmox box (min envelope 2 vCPU /
   4 GiB RAM / 20 GiB disk — your node is well above that).
2. Clock discipline: chrony or systemd-timesyncd holding skew <2 s.
   The chain enforces MAX_FUTURE_DRIFT = 2 s, and our T13 audit just
   caught one host running 83 s behind NTP — so we check this for
   real before onboarding.
3. Stable uptime + an ops contact we can ping when your seat alarms.
4. Run our vahana rc10 binary from the onboarding doc —
   one-command install, config bundle provided.

Mainnet: honest answer, no date yet. G2 runs until the gates hold
(sustained finality under churn, cross-chain light-client demos,
PQ hardening), then mainnet. The milestone roadmap gets published
alongside the G2 launch docs.

Onboarding pack + endpoints land within days of G1 close-out.
Hold tight — you get the first ping.

---
FOLLOW-UP PING (optional, pre-close-out — puts him to work now
without sending a pack that doesn't exist yet):

Quick heads-up: G1 closes tomorrow ~20:00 UTC on schedule — 48 h
soak, verdict + announcement right after, then G2 opens and
external onboarding is the first move. Still first on the list.

Two things you can do NOW that have real lead time, so when the
pack lands you're a zero-friction onboard:

1. Spin the dedicated LXC/VM (2 vCPU / 4 GiB / 20 GiB minimum —
   4/8/50 recommended if the box can spare it).
2. Run the clock self-audit: `chronyc tracking` (or `timedatectl`)
   — RMS offset should be well under 1 s. Our chain rejects
   proposals stamped >2 s ahead of local clock, and our own G1
   audit caught an 83 s drift on OUR fleet, so this one's real,
   not ceremony.

That's the whole prep. Pack + config bundle follows within days
of close-out — you'll be the first external seat through the
state-sync join path.
