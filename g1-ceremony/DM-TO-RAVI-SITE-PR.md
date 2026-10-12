To: Ravi — PR #1 review + vahana ops numbers (paste-ready DM)

Ravi — went through PR #1 properly. Genuinely good work: the
product-first hero, three modes of agency, four witnesses, and the
honest LIVE / DEVNET / PLANNED status split — keep all of it.

Your own last comment is the one real issue: the PR overcorrects
and skips the blockchain. For JunoClaw the chain isn't trivia —
it's the verification layer that makes "verifiable" real:

- sub-2 s BLS-threshold finality (measured ~2 s/blk steady state;
  leader-timeout spikes quantified in the soak)
- MAYO2 PQ-hybrid certificates on every block (616 B hybrid
  envelope, 3-of-4 signer quorum)
- an on-chain light-client contract — 08-wasm, deployed on
  junoclaw-g1, positive + negative verification paths both passing
  at h508995
- airdrop-claim + governance flow tested end-to-end on-chain

No other agent stack can point at that. So: keep consensus out of
the hero (you had that right), but add an "Under the hood" section
with those numbers as evidence. That's the differentiator section.

Plan: don't merge yet — keep the PR open as the base. We close G1
right after the soak (48 h mark ~2026-10-13 20:00 UTC) and update
the site immediately after: your branch + the chain section with
the real G1 numbers.

Also — ops ask for vahana (soak close-out, any time before
2026-10-13 20:00 UTC):
1. `nproc; free -m; df -h <vahana data dir>` — T14 resource audit;
   your box is the only unconfirmed seat.
2. Clock offset: `timedatectl` or `chronyc tracking` — T13 needs
   <2 s; on-chain we already measured ovh-uk1↔vahana at ±100 ms,
   this just confirms host-side.
3. WARN/ERROR counts in vahana logs since 2026-10-11 ~19:58 UTC
   (docker logs / journalctl) — vahana is one of only two live
   proposers right now, so your log is the best source for
   attributing the leader-timeout pattern.
4. Service status + uptime confirmation.

Thanks — the PR is 80% of what I wanted; the chain section is the
missing 20%.
