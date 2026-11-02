# RC18 — certified-but-unfinalized parent deadlock (the wedge cure)

Status: design. Interim mitigation shipped: RC17v2 (wipe-restart
automation). This doc designs the protocol-level fix.

## The failure

Simplex-style flow: leader proposes → quorum notarizes (certificate) →
quorum finalizes → executed. The 2026-11-01 wedge: a payload was
**certified but never finalized** (quorum dropped mid-view). Leaders
correctly built on the notarized tip; every seat's `verify()` rejected
proposals whose parent it had never executed — "executed parent
unavailable" — for ~980k views.

The invariant that creates the trap (node.rs, rc14 comment):

> ONLY a Finalization activity may advance execution. A certified view
> is NOT unique — under view churn two different views can certify two
> different same-height payloads with zero equivocation (that forked
> g1 at h2631739: 4c03cc6b… vs 2039d342…, identical app_hash).

So: a certified parent is ambiguous (could be a dead variant) but a
finalized payload is unique. **"Just execute the certified parent" is
provably wrong** — that is the rc6 bug, documented in the code.

## Why the wedge can't self-heal today

- The certified payload's own Finalization needed votes in its *own*
  view — that view ended without quorum. Finalization can't be
  manufactured retroactively.
- New views propose children of the *notarized tip* (the dead variant)
  — parent selection follows notarization, not execution.
- verify() requires `parent == executed tip` → every child of the dead
  variant fails → notarization can never advance past it.
- Only a cohort-wide *forget* (wipe consensus state) resets the
  notarized tip. RC17v2 automates that forget, per-seat.

## Candidate mechanisms

### M1 — deferred finalization (preferred if upstream allows)

A fresh quorum emits **Finalize activity for an already-certified
payload** in a later view. The certificate already proves quorum
attestation — finalizing it afterwards adds no new trust; execution
then proceeds on the normal finalized path. No speculative execution,
no parent-selection change.

- Requirements: consensus must allow Finalize votes targeting a
  payload certified in an *earlier* view (Finalization bound to
  (height, digest), not (view, digest)).
- Risk: low — finalization remains unique per height.
- Cost: upstream change (commonware). File with the 2026-11-01
  evidence: certified digest `6faf9ba7…` at h3938615, ~980k dead views.

### M2 — dead-variant abandonment (app-side, if propose() chooses parent)

After N verify-failures on the same parent, leaders build on the
**executed tip** instead of the notarized tip. The dead variant is
orphaned; a sibling branch notarizes/finalizes normally (its parent IS
executed → verify passes).

- Requirements: the app must influence proposal parentage — likely an
  upstream hook; commonware picks the parent from the notarized set.
- Risk: forks-by-construction — two parents competing at same height
  is legal Simplex; the finalized chain still picks one winner, but
  sibling proposals raise equivocation-adjacent surface. Requires the
  abandonment threshold to be *identical across seats* (deterministic
  rule: abandon when certified_age ≥ K views) or the cohort splits.
- Verdict: viable but heavier; prefer M1.

### M3 — coordinated forget escalation (what RC17v2 does, distributed)

The wipe-restart is already cohort-coordinated abandonment. A
protocol-level version: a signed **dead-variant marker** broadcast
app-side; a quorum of markers → all seats drop the certified payload
from the notarized set simultaneously (same effect as RC17v2 without
the restart).

- Requirements: marker kind + quorum threshold + notarized-set edit —
  upstream change again, but smaller than M1.
- Verdict: RC17v2 already delivers 90% of the value; M3 is the graceful
  version of the same idea.

## Recommendation

**File M1 upstream with the incident evidence; keep RC17v2 as the
distributed interim.** M2/M3 only if M1 is rejected. Non-negotiables:

- Never execute a merely-*notarized*/certified ancestor — the h2631739
  fork exists because that once happened.
- Finalization remains the only execution trigger; every mechanism
  above ends in a *finalized* payload executing — none bypass it.
- Any fix must not weaken the `verify()` executed-parent check for
  normal (non-wedge) proposals.

## Evidence bundle for the upstream issue

- Dead certified digest `6faf9ba7…`, height 3938615, chain junoclaw-g1
- Executed tip: `464e35b2a92e…38bf3` at same height — parent never
  finalized
- ~980k views of leader→verify-reject→nullify churn
- 1017 CLOSE-WAIT residue documented in the incident postmortem
- RC17v2 recovery semantics (marker dedupe, 2/hr cap) as the interim
  cohort-coordinated workaround
