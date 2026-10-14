# DM to Anatoly — G2 validator readiness (send before G2 opens)

Supersedes the earlier invitation draft — this is the version
to send, aligned with the post-call plan (batched onboarding at
set-change windows, failure-domain rule added post-incident).

---

Anatoly — timing update since you asked on the group.

G1 (closed ceremony soak) completes tonight ~19:58 UTC. After a
short hardening window we open G2 — the public testnet phase —
and validator onboarding happens at the FIRST set-change window
once the baseline is green (days, not a single moment;
operators join in batches).

To be ready, your seat needs:

1. Dedicated VM/LXC — 2 vCPU/4 GB/20 GB min (4/8/50 better),
   on infrastructure YOU control, independent host+network
   (one hard rule from our soak: no two seats may share a
   host or ISP — that's what cost us a halt this week).
2. Clock discipline — chrony or systemd-timesyncd synced,
   drift <2 s. Our authenticated P2P rejects stale clocks.
3. Stable uptime + a reachable P2P port (7001).
4. A named ops contact who answers a test-page.

What I need back from you when ready:

- Machine spec + provider + region
- Static IP / DNS for the peer list
- Your ops contact (TG or email)
- OS (Ubuntu preferred)

Keys stay yours — you generate Ed25519 + MAYO2 locally during
the ceremony; nothing sensitive ever transits. I'll send the
full onboarding doc the moment the G2 window opens.
