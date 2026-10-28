#!/bin/bash
# conformance.sh — shadow-run conformance diff: Buzz vs claw-relay.
#
# Drives the SAME probe sequence at both relays over the shared Nostr wire
# protocol and diffs behaviour. buzz-cli is the probe client — it is the
# canonical consumer, so "buzz-cli behaves identically" IS the conformance
# bar for cutover.
#
# Usage:
#   ops/claw-relay/conformance.sh \
#       --buzz wss://relay.junoclaw.xyz --claw ws://127.0.0.1:7070 \
#       --channel <uuid> --key-file /opt/buzz/state/status-bot.sec \
#       [--window-secs 600] [--out conformance-$(date +%Y%m%d).txt]
#
# Probes (each reported MATCH / DIFF / SKIP per relay pair):
#   R1 read-replay    : messages get over the same [window] — event-id sets
#   R2 read-order     : events ordered by (created_at,id) on both
#   W1 write-accept   : signed EVENT accepted by both (key must be
#                       allowlisted on BOTH relays to get MATCH)
#   W2 write-dedupe   : resend same event -> still OK (dup tolerated)
#   N1 forged-sig     : EVENT with zeroed sig -> OK false       [raw ws]
#   N2 outsider       : valid sig, non-allowlisted -> OK false  [raw ws]
#   N3 bad-channel    : h-tag channel not configured -> OK false[raw ws]
#   N4 bad-kind       : kind outside admitted set -> OK false   [raw ws]
#   N5 eose           : REQ returns EOSE terminator             [raw ws]
#
# Raw-WS probes need `websocket-client` for python3; skipped gracefully if
# absent. Writes are only attempted when --key-file is given (its npub must
# be allowlisted on claw-relay; for the Buzz leg it must also be on the Buzz
# community allowlist — W1 reports per-relay outcomes either way).
#
# Read window note: use --window-secs covering a period where BOTH relays
# were ingesting (i.e. after shadow-start). claw-relay has no history
# before it ran — comparing all-time will always DIFF and means nothing.
set -u
B=$(dirname "$0")
BUZZ_URL=${BUZZ_URL:-wss://relay.junoclaw.xyz}
CLAW_URL=${CLAW_URL:-ws://127.0.0.1:7070}
CLI=${BUZZ_CLI:-/opt/buzz/bin/buzz-cli}
KEYFILE=""
CHAN=""
WIN=600
OUT=""

while [ $# -gt 0 ]; do
  case "$1" in
    --buzz) BUZZ_URL=$2; shift 2;;
    --claw) CLAW_URL=$2; shift 2;;
    --channel) CHAN=$2; shift 2;;
    --key-file) KEYFILE=$2; shift 2;;
    --window-secs) WIN=$2; shift 2;;
    --out) OUT=$2; shift 2;;
    *) echo "unknown arg: $1"; exit 1;;
  esac
done
[ -n "$CHAN" ] || { echo "--channel <uuid> required"; exit 1; }
[ -n "$OUT" ] || OUT="conformance-$(date -u +%Y%m%dT%H%M).txt"

NOW=$(date +%s); FROM=$((NOW - WIN)); TO=$NOW
say()  { echo "$*" | tee -a "$OUT"; }
rep()  { say "$(printf '%-22s buzz=%-42s claw=%-42s %s' "$1" "$2" "$3" "$4")"; }

say "claw-relay conformance — $(date -Is)"
say "buzz=$BUZZ_URL claw=$CLAW_URL channel=$CHAN window=[$FROM,$TO)"
say "---"

envx() { # $1=relay url; rest=buzz-cli args
  local url=$1; shift
  if [ -n "$KEYFILE" ]; then
    env BUZZ_PRIVATE_KEY="$(sudo cat "$KEYFILE")" BUZZ_RELAY_URL="$url" "$CLI" "$@"
  else
    env BUZZ_RELAY_URL="$url" "$CLI" "$@"
  fi
}

# --- R1/R2: read replay -------------------------------------------------
ids_b=$(envx "$BUZZ_URL" messages get --channel "$CHAN" --since "$FROM" --before $((TO-1)) --limit 500 2>/dev/null \
        | python3 -c "import json,sys;print('\n'.join(sorted(e['id'] for e in json.load(sys.stdin))))" 2>/dev/null)
ids_c=$(envx "$CLAW_URL" messages get --channel "$CHAN" --since "$FROM" --before $((TO-1)) --limit 500 2>/dev/null \
        | python3 -c "import json,sys;print('\n'.join(sorted(e['id'] for e in json.load(sys.stdin))))" 2>/dev/null)
nb=$(echo "$ids_b" | grep -c . || true); nc=$(echo "$ids_c" | grep -c . || true)
[ "$ids_b" = "$ids_c" ] && rep "R1 read-id-set" "$nb events" "$nc events" "MATCH" \
                        || rep "R1 read-id-set" "$nb events" "$nc events" "DIFF (claw shadow window? check --window-secs)"

# --- W1/W2: write accept + dedupe (optional) ----------------------------
if [ -n "$KEYFILE" ]; then
  TAG="conformance-probe-$(date +%s)"
  r_b=$(envx "$BUZZ_URL" messages send --channel "$CHAN" --content "$TAG" 2>&1 | tail -1)
  r_c=$(envx "$CLAW_URL" messages send --channel "$CHAN" --content "$TAG" 2>&1 | tail -1)
  rep "W1 write-accept" "${r_b:0:40}" "${r_c:0:40}" "$([ "${r_b:0:5}" = "${r_c:0:5}" ] && echo MATCH || echo 'SEE DETAIL')"
fi

# --- N-probes: raw websocket --------------------------------------------
if python3 -c "import websocket" 2>/dev/null; then
  PY="$B/ws-probe.py"
  [ -f "$PY" ] || { say "NOTE: ws-probe.py missing — N-probes skipped"; exit 0; }
  for probe in forged outsider badchan badkind eose; do
    # outsider must NOT get --key-file — it probes with the fixed
    # non-member key by definition.
    KF=""; [ "$probe" != "outsider" ] && KF="${KEYFILE:+--key-file $KEYFILE}"
    b=$(python3 "$PY" --url "$BUZZ_URL" --channel "$CHAN" --probe "$probe" $KF 2>/dev/null || echo "ERR")
    c=$(python3 "$PY" --url "$CLAW_URL" --channel "$CHAN" --probe "$probe" $KF 2>/dev/null || echo "ERR")
    rep "N $probe" "$b" "$c" "$([ "$b" = "$c" ] && echo MATCH || echo DIFF)"
  done
else
  say "N-probes skipped: python3 websocket-client not installed"
fi
say "---"
say "report: $OUT"
