To: Ravi — RAM follow-up + disk resolved (paste-ready reply)

Ravi — both follow-ups closed, thank you. Logging as follows:

1. Disk 95% -> 83% (13 GiB free) — flag closed. The G2 >=20 GiB
   free bar still wouldn't hold on that root, so your "new
   machine for next testnets" plan is the right fix — a
   dedicated box clears the G2 seat bar (2c/4G/20G + clock
   <2 s) trivially. We'll send the G2 onboarding doc at soak
   close-out.

2. RAM — accepted, and thank you for the four-way cross-check.
   We log vahana at ~1.0 GiB resident / 1.6 GiB peak (cgroup
   memory.peak) and strike the 76 MB as a boot-time misread on
   your side — no cgroup bug, corrected in the record.

   To answer your question: our 2.15 GiB is NOT host-wide. The
   slay3rd on our box runs systemd-native (no container), and
   the number is the RSS column of `ps aux` on the slay3rd
   process itself — the same VmRSS view as your /proc check,
   no page-cache or co-tenant inflation in it. So the delta is
   real process-RSS between the two seats, and your hypothesis
   (1) fits what we see: our box has 94 d uptime vs your
   container's 35 h, and our seat proposes ~half of all blocks,
   so a larger rocksdb/heap working set is plausible. I'll send
   a fresh VmRSS + VmHWM + cgroup current/peak read from our
   side to sit beside yours in the T14 table — the ssh window
   is tight mid-soak, so it lands at close-out with the rest
   of the numbers.

Net for T14: vahana = 1.0 GiB resident / 1.6 GiB peak, disk
83%, clock 74 us — clean pass, nothing further needed from you
before close-out (~2026-10-13 20:00 UTC). Until then: no
restarts, no config changes, let it run.
