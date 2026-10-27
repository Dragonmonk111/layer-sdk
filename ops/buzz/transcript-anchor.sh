#!/bin/bash
# transcript-anchor.sh — Merkle-root each Buzz channel's message window and
# anchor the root on-chain as a moultbook entry, then post the receipt back
# to the channel. Makes deliberation tamper-evident before claw-relay
# exists; the on-chain entry's `refs` chains each anchor to the previous
# one, so dropped history breaks the chain.
#
# Window rule (JC-TRANSCRIPT-ANCHOR-v1): contiguous half-open CLOCK ranges
#   [next_from_ts, now - ANCHOR_LAG_SECS). The lag gives late-delivered
#   events a chance to land inside their true window; an event whose
#   created_at falls inside an already-anchored range is never covered —
#   the anchor binds what the relay served, it cannot rewrite it.
#
# Cron: hourly (e.g.  17 * * * *)
#
# Requires: buzz-cli, bls-relayer, curl, python3, openssl, jq-free (python
#           does JSON); /opt/dao-keys/dao-key.pem, $B/status-bot.sec.
# Committed-state discipline like buzz-vote-daemon: the receipt is posted
# only after the entry is read back from chain state — never on
# "broadcast OK".
set -e
B=${B:-/opt/buzz/state}
GRPC=${GRPC:-127.0.0.1:9090}
API=${API:-http://127.0.0.1:8082}
MOULTBOOK=${MOULTBOOK:-juno1xh8rfpsnh4ylkpr6yanpy2y9903yq2ugr7ycry59p84r5c5pethqua7wum}
ANCHOR_LAG_SECS=${ANCHOR_LAG_SECS:-300}        # anchor up to now-5min
BOOTSTRAP_HOURS=${BOOTSTRAP_HOURS:-24}         # first run covers last 24h
MAX_SIZE=${MAX_SIZE:-60000}                    # < contract max_size_bytes 65536
CONTENT_TYPE="x-junoclaw/transcript-anchor"
SEAT=$(cat $B/my-juno1.txt 2>/dev/null || true)
DRY=0; [ "${1:-}" = "--dry-run" ] && DRY=1

# name:uuid pairs — anchored every run, in order.
CHANNELS=${ANCHOR_CHANNELS:-"governance:2a6c9ad2-9da8-406c-8aa3-cd2f1e33af38 validator-ops:810d3c45-acb5-44f8-9155-8bc94b3a12b3 agents:7b4f3765-8494-4de8-bf7a-b2595da827e0"}

buzz() { # read path — bot identity only used for auth to the relay
  sudo env BUZZ_PRIVATE_KEY="$(sudo cat $B/status-bot.sec)" \
    BUZZ_RELAY_URL=wss://relay.junoclaw.xyz /opt/buzz/bin/buzz-cli "$@"
}
post() { # $1=channel uuid, stdin=content
  printf "%b" "$(cat)" | sudo env BUZZ_PRIVATE_KEY="$(sudo cat $B/status-bot.sec)" \
    BUZZ_RELAY_URL=wss://relay.junoclaw.xyz \
    /opt/buzz/bin/buzz-cli messages send --channel "$1" --content - >> $B/anchor.log 2>&1
}

NOW=$(date +%s)
TO=$((NOW - ANCHOR_LAG_SECS))

for PAIR in $CHANNELS; do
  NAME=${PAIR%%:*}; CH=${PAIR##*:}
  ST=$B/anchor-$NAME.json
  if [ -f "$ST" ]; then
    FROM=$(python3 -c "import json;print(json.load(open('$ST'))['next_from_ts'])")
    PREV_ENTRY=$(python3 -c "import json;print(json.load(open('$ST')).get('last_entry') or '')")
  else
    FROM=$((NOW - BOOTSTRAP_HOURS * 3600)); PREV_ENTRY=""
  fi
  [ "$TO" -le "$FROM" ] && { echo "$(date -Is) $NAME: nothing new yet" >> $B/anchor.log; continue; }

  # fetch the whole window, paging by --before until a short page returns
  MSGS="[]"; CURSOR=$TO
  for i in 1 2 3 4 5 6 7 8 9 10; do
    PAGE=$(buzz messages get --channel "$CH" --since "$FROM" --before $((CURSOR - 1)) --limit 200 2>/dev/null) || break
    N=$(echo "$PAGE" | python3 -c "import json,sys;print(len(json.load(sys.stdin)))" 2>/dev/null) || break
    [ "$N" -eq 0 ] && break
    MSGS=$(python3 -c "import json,sys;a=json.loads(sys.argv[1]);b=json.loads(sys.argv[2]);print(json.dumps(a+b))" "$MSGS" "$PAGE")
    [ "$N" -lt 200 ] && break
    CURSOR=$(echo "$PAGE" | python3 -c "import json,sys;print(min(e['created_at'] for e in json.load(sys.stdin)))")
  done
  CNT=$(echo "$MSGS" | python3 -c "import json,sys;print(len(json.load(sys.stdin)))" 2>/dev/null || echo 0)

  if [ "$CNT" -eq 0 ]; then
    # empty window: no anchor, but still advance the clock cursor so ranges
    # stay uniform and verifiable (never in --dry-run: probes don't move state)
    [ "$DRY" -eq 0 ] && python3 -c "import json;json.dump({'next_from_ts':$TO,'last_entry':'$PREV_ENTRY' or None},open('$ST','w'))"
    echo "$(date -Is) $NAME: empty window $FROM..$TO" >> $B/anchor.log
    continue
  fi

  PKG=$(echo "$MSGS" | python3 "$(dirname "$0")/transcript-anchor.py" compute \
        --max-size "$MAX_SIZE" --to-ts "$TO") || { echo "$(date -Is) $NAME: compute failed" >> $B/anchor.log; continue; }
  read ROOT_HEX ROOT_B64 WFROM WTO WCNT WSIZE TRUNC < <(echo "$PKG" | python3 -c \
    "import json,sys;p=json.load(sys.stdin);print(p['root_hex'],p['root_b64'],p['from_ts'],p['to_ts'],p['count'],p['size_bytes'],p['truncated'])")

  REFS="[]"; [ -n "$PREV_ENTRY" ] && REFS="[\"$PREV_ENTRY\"]"
  # visibility is a serde unit variant: the wire form is the STRING "public",
  # not {"public":{}} — map form fails ExecuteMsg parsing (verified on-chain)
  MSG=$(python3 -c "import json;print(json.dumps({'post':{'commitment':'$ROOT_B64','content_type':'$CONTENT_TYPE','size_bytes':$WSIZE,'attestation_ref':None,'visibility':'public','refs':$REFS}}))")

  if [ "$DRY" -eq 1 ]; then
    echo "[dry-run] $NAME win $WFROM..$WTO msgs=$WCNT root=$ROOT_HEX refs=$REFS"
    continue
  fi

  RES=$(JUNOCLAW_KEY_HEX="$(sudo openssl ec -in /opt/dao-keys/dao-key.pem -text -noout 2>/dev/null \
        | awk '/^priv:/{f=1;next}/^pub:/{f=0}f' | tr -d ' :\n')" \
        bls-relayer jc-execute --contract "$MOULTBOOK" --msg "$MSG" \
        --layer-grpc "$GRPC" 2>&1) || true
  TXH=$(echo "$RES" | grep -oE '[0-9A-F]{64}' | head -1)

  # committed-state verification: find OUR entry by commitment via
  # list_by_author — an anchor is only real once it reads back from state.
  ENTRY=""
  if [ -n "$SEAT" ]; then
    for i in 1 2 3 4 5 6; do
      sleep 5
      Q=$(python3 -c "import base64,json;print(base64.urlsafe_b64encode(json.dumps({'list_by_author':{'author':'$SEAT','limit':20}}).encode()).decode().rstrip('='))")
      ENTRY=$(curl -fsS -m 10 "$API/api/wasm/smart/$MOULTBOOK?query=$Q" 2>/dev/null | python3 -c \
        "import json,sys,base64
try:
  es=json.load(sys.stdin)['data']['entries']
  m=[e for e in es if e.get('commitment')=='$ROOT_B64']
  print(m[0]['id'] if m else '')
except Exception: print('')" 2>/dev/null)
      [ -n "$ENTRY" ] && break
    done
  fi

  if [ -n "$ENTRY" ]; then
    python3 -c "import json;json.dump({'next_from_ts':$WTO,'last_entry':'$ENTRY'},open('$ST','w'))"
    post "$CH" <<EOF
[anchor] #$NAME transcript root anchored on-chain
window ${FROM}→${WTO} · msgs ${WCNT}$( [ "$TRUNC" = "True" ] && echo " (split — remainder next anchor)" )
root ${ROOT_HEX}
entry ${ENTRY} · tx ${TXH:-?}$( [ -n "$PREV_ENTRY" ] && echo " · prev ${PREV_ENTRY}" )
verify: buzz messages get --channel $CH --since $FROM --before $((WTO - 1)) | python3 transcript-anchor.py verify --root $ROOT_HEX
EOF
    echo "$(date -Is) $NAME anchored win $FROM..$WTO msgs=$WCNT entry=$ENTRY tx=$TXH" >> $B/anchor.log
  else
    # do NOT advance the cursor — next run retries the same window
    echo "$(date -Is) $NAME FAILED commit win $WFROM..$WTO tx=${TXH:-none}: $(echo "$RES" | tail -1 | head -c 160)" >> $B/anchor.log
  fi
done
exit 0
