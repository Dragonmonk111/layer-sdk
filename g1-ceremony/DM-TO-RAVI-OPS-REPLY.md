To: Ravi — T13/T14 confirmation + your two flags (paste-ready reply)

Ravi — this is exactly what close-out needed, thank you. Your
numbers close the last open T13/T14 caveats on your seat:

- T13 PASS: 73 µs slow / RMS offset 74.6 µs against
  ntp-nts-3.ps5.canonical.com, stratum 3, NTS active — best-
  measured clock in the fleet, and consistent with the ±100 ms
  bound we measured on-chain between your seat and ours.
  Nothing to do here.
- T14 PASS on CPU (4c) and RAM provisioning (7.7 GiB total,
  2.6 GiB available — shared box, tight but fine for the
  node's actual usage). Two follow-ups, neither chain-affecting
  today:
  1. Disk: / is at 95% (4.2 GiB free) and the chain data dir
     sits on it. Please send `du -sh` of the data dir and get /
     under ~85% — at G2 we require ≥20 GiB free on a seat, and
     a 95%-full host root is exactly what a G2 seat audit fails.
  2. Your ~76 MB container RAM vs the ~2.2 GiB resident we
     measure via ssh on our equivalent seat smells like a
     page-cache/cgroup accounting gap. Send
     `docker stats --no-stream junoclaw-g1` so we log
     apples-to-apples.

On your two flags:

1. Block rate — real, but a window artifact rather than a
   trend. Our independent monitor samples all three gRPC seats
   every 30 s: 14.3 h mean 3.79 s/blk, hourly means swinging
   2.8-5.7 s — the 08:00 UTC hour was the soak low-water mark
   at ~5.7 s/blk — and the last 15 minutes are back at ~3.6.
   Short windows read wild because commits arrive in bursts
   (adjacent-block deltas 0-89 ms) separated by 4.7-86 s
   view-skip cascades: 10 blocks in 2 minutes can read 12 s/blk
   while the hour holds ~5. Not progressive — the low-water
   hour was followed by recovery, zero stalls >90 s, zero
   divergence, all seats at identical tips at every sample.
   The remedy is the queued rc10 peering fix; nothing to change
   mid-soak.

2. The 994 WARNs are the expected signature, not a symptom. Our
   on-chain leader histogram (h514230-514389) shows only two of
   four seats ever land proposals — your seat and ours; the
   akash + home seats' leader slots always expire. Those 933
   nullification-floor broadcasts ARE the skip mechanism for
   the dead slots (the Finalization/Notarization floor split
   just reflects what the skipped view was sitting on), and the
   61 view/round mismatches are stale proposals from
   already-skipped views being correctly rejected. Zero ERRORs
   is the signal we care about, and you're clean there. All
   three WARN classes disappear when peering is fixed in rc10.

Saw you applied the PR changes — I'll re-review against the
notes this week, and we merge right after close-out
(~2026-10-13 20:00 UTC) so the chain section carries the final
48 h numbers.

Until then: no restarts, no config changes, let it run.
