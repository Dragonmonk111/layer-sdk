#!/bin/bash
# devnet wedge soak — A/B test of dead-variant recovery on real binaries.
#
# Usage: soak.sh <binary> [rounds]
#   Starts 4 nodes on 127.0.0.1 (seed-0 devnet keys), waits for
#   finalization, then churns: SIGSTOP two seats for a few seconds and
#   resume. Any pause that lands inside a view's notarize->finalize gap
#   produces a certified-but-unfinalized "dead variant" — the exact
#   2026-11-04 production wedge.
#
#   rc17 (control): the wedge permanently stalls the chain — verify()
#   rejects descendants of the unexecuted certified parent.
#   rc18: staged verification lets the descendant notarize+finalize,
#   pulling the dead variant into finality. Height resumes advancing.
#
# Signals watched:
#   alive  — "Block finalized with BLS threshold certificate" height N
#   wedge  — "consensus parent certified but not executed"
#   escape — "staged_parent: simulated certified-but-unexecuted parent"

set -uo pipefail

BIN="$1"
ROUNDS="${2:-25}"
SOAK=/tmp/jcl-soak

height() { sed 's/\x1b\[[0-9;]*m//g' "$SOAK"/node*/node.log 2>/dev/null | grep -o 'Block finalized with BLS threshold certificate.*height=[0-9]*' | grep -o 'height=[0-9]*' | tail -1 | cut -d= -f2; }
staged() { grep -c 'staged_parent: simulated' "$SOAK"/node*/node.log 2>/dev/null | awk -F: '{s+=$2} END{print s+0}'; }
wedge() { grep -c 'certified but not executed' "$SOAK"/node*/node.log 2>/dev/null | awk -F: '{s+=$2} END{print s+0}'; }

stop_all() {
    for i in 0 1 2 3; do
        [ -f "$SOAK/node$i/node.pid" ] && kill "$(cat "$SOAK/node$i/node.pid")" 2>/dev/null
    done
}
trap stop_all EXIT

# --- launch -----------------------------------------------------------------
for i in 0 1 2 3; do
    rm -rf "$SOAK/node$i/data" "$SOAK/node$i/node.log"
    mkdir -p "$SOAK/node$i/data"
    RUST_LOG=info "$BIN" "$SOAK/node$i/node.toml" > "$SOAK/node$i/node.log" 2>&1 &
    echo $! > "$SOAK/node$i/node.pid"
done
sleep 3
for i in 0 1 2 3; do
    kill -0 "$(cat "$SOAK/node$i/node.pid")" 2>/dev/null || { echo "LAUNCH-FAIL node$i"; tail -5 "$SOAK/node$i/node.log"; exit 1; }
done
echo "LAUNCH-OK bin=$(basename "$BIN")"

# --- wait for baseline finalization ------------------------------------------
deadline=$((SECONDS + 90))
h0=0
while [ $SECONDS -lt $deadline ]; do
    h0=$(height); h0=${h0:-0}
    [ "$h0" -ge 5 ] && break
    sleep 2
done
echo "BASELINE height=$h0 staged=$(staged) wedge=$(wedge)"
[ "$h0" -ge 5 ] || { echo "NO-FINALIZATION — aborting"; tail -5 "$SOAK/node0/node.log"; exit 1; }

# --- churn loop ---------------------------------------------------------------
wedges_seen=0
stall_events=0
for r in $(seq 1 "$ROUNDS"); do
    hpre=$(height); hpre=${hpre:-0}
    p2=$(cat "$SOAK/node2/node.pid"); p3=$(cat "$SOAK/node3/node.pid")
    if [ $((r % 2)) -eq 0 ]; then
        kill -STOP "$p2" 2>/dev/null; sleep 1; kill -STOP "$p3" 2>/dev/null   # staggered
    else
        kill -STOP "$p2" "$p3" 2>/dev/null                                   # simultaneous
    fi
    sleep $((3 + RANDOM % 4))          # straddle a view + timeout
    kill -CONT "$p2" "$p3" 2>/dev/null
    sleep 15                          # recovery window (recommended paces re-dials ~1/min)
    hpost=$(height); hpost=${hpost:-0}
    w=$(wedge); s=$(staged)
    if [ "$hpost" -le "$hpre" ]; then
        stall_events=$((stall_events + 1))
        echo "round $r: STALL height=$hpost wedge-lines=$w staged=$s"
    else
        echo "round $r: advanced $hpre -> $hpost wedge-lines=$w staged=$s"
    fi
done

echo "=== SOAK RESULT bin=$(basename "$BIN") final=$(height) stalls=$stall_events/$ROUNDS staged=$(staged) wedge=$(wedge) ==="
