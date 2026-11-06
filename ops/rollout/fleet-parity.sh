#!/usr/bin/env bash
# fleet-parity.sh — coordinator-side digest parity across reported seats.
# Digest = full block identity (not height, not app_hash). GREEN only when
# every reporting seat produced the SAME digest at a common height.
#
#   ./fleet-parity.sh reports.txt [--min-seats 10] [--min-heights 3]
#
# reports.txt: paste each seat's digest-report.sh output, one blob per seat:
#   SEAT ovh-uk1 4330100 4a2f4e4d498667c3
#   SEAT home-vb 4330100 4a2f4e4d498667c3
set -euo pipefail

REPORTS="${1:?usage: fleet-parity.sh reports.txt [--min-seats N] [--min-heights N]}"
MIN_SEATS=10
MIN_HEIGHTS=3
shift
while [ $# -gt 0 ]; do
    case "$1" in --min-seats) MIN_SEATS="$2"; shift 2;;
                 --min-heights) MIN_HEIGHTS="$2"; shift 2;;
                 *) echo "unknown flag $1" >&2; exit 2;; esac
done

PYBIN=""
for c in python3 python; do
    if command -v "$c" >/dev/null 2>&1 && "$c" -c 'import sys' >/dev/null 2>&1; then PYBIN="$c"; break; fi
done
[ -n "$PYBIN" ] || { echo "no working python found" >&2; exit 2; }

"$PYBIN" - "$REPORTS" "$MIN_SEATS" "$MIN_HEIGHTS" <<'PY'
import re, sys, collections
path, min_seats, min_heights = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
seats = {}      # seat -> {height: digest}
for line in open(path, encoding='utf-8', errors='replace'):
    m = re.match(r'SEAT\s+(\S+)\s+(\d+)\s+([0-9a-fA-F]{16})', line.strip())
    if not m: continue
    seat, h, d = m.group(1), int(m.group(2)), m.group(3).lower()
    seats.setdefault(seat, {})[h] = d
    if len(seats[seat]) < len(set(seats[seat])): pass

names = sorted(seats)
print(f"seats reporting: {len(names)} -> {', '.join(names)}")
# common heights = heights reported by every seat
common = set.intersection(*[set(s) for s in seats.values()]) if seats else set()
common = sorted(common)
mismatch = []
agree = 0
for h in common:
    ds = {seats[n][h] for n in names}
    if len(ds) == 1: agree += 1
    else:
        mismatch.append((h, {n: seats[n][h] for n in names}))
print(f"common heights: {len(common)}  agreeing: {agree}  DIVERGENT: {len(mismatch)}")
for h, dd in mismatch:
    print(f"  !! h{h}:")
    for n in names: print(f"      {n:16s} {dd[n]}")
per_seat_cov = {n: len(seats[n]) for n in names}
print("coverage:", ", ".join(f"{n}={c}" for n, c in per_seat_cov.items()))

if len(names) < min_seats:
    print(f"PARITY-BLOCKED: only {len(names)}/{min_seats} seats reported"); sys.exit(1)
if mismatch:
    print("PARITY-RED: digest divergence — DO NOT greenlight, investigate diverging seat")
    sys.exit(2)
if agree < min_heights:
    print(f"PARITY-WAIT: only {agree} common heights (need {min_heights}) — wait for more reports")
    sys.exit(3)
print(f"PARITY-GREEN: {len(names)} seats agree on digest at {agree} common heights")
PY
