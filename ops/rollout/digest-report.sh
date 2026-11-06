#!/usr/bin/env bash
# digest-report.sh — emit canonical height→digest lines from THIS seat's log.
# Paste output into the fleet thread; the coordinator diffs across seats.
#
#   ./digest-report.sh                 # last 20 finalized blocks
#   ./digest-report.sh 4330100         # single height
#   ./digest-report.sh 4330100 4330110 # range
#
# Env: LOG_CMD (defaults to journald; set per-seat, e.g.
#   LOG_CMD='grep -a . /var/log/slay3rd.log'  or  'tail -100000 /path/log')
set -euo pipefail
LOG_CMD="${LOG_CMD:-journalctl -u junoclaw-g1 -n 50000 --no-pager}"
SEAT="${SEAT_NAME:-$(hostname -s 2>/dev/null || hostname)}"

emit() { # emit HEIGHT if in requested range
    local h="$1" d="$2"
    case "${MODE:-last}" in
        one)   [ "$h" = "$WANT_A" ] && echo "SEAT $SEAT $h $d" ;;
        range) [ "$h" -ge "$WANT_A" ] && [ "$h" -le "$WANT_B" ] && echo "SEAT $SEAT $h $d" ;;
        last)  echo "SEAT $SEAT $h $d" ;;
    esac
    return 0
}

if [ $# -eq 1 ]; then MODE=one; WANT_A="$1"; WANT_B="$1"
elif [ $# -eq 2 ]; then MODE=range; WANT_A="$1"; WANT_B="$2"
else MODE=last; fi

$LOG_CMD 2>/dev/null \
  | sed -E 's/\x1b\[[0-9;]*m//g' \
  | grep -a 'Block finalized' \
  | sed -nE 's/.*height[^0-9]*([0-9]{4,}).*digest=([0-9a-f]{16}).*/\1 \2/p' \
  | { [ "$MODE" = last ] && tail -20 || cat; } \
  | while read -r h d; do emit "$h" "$d"; done
