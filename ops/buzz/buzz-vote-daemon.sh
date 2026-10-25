#!/bin/bash
# buzz-vote-daemon.sh — watch #governance for vote intents signed by THIS
# seat's member npub; cast the vote on-chain with the local governance key,
# then post a confirmation (with txhash) back to the channel.
#
# Auth model: the relay already verified each message's Nostr signature —
# the pubkey field IS the authenticated member identity. The daemon only
# acts on messages authored by $NPUB_HEX (this seat's bound hot key).
#
# Intent format:  !vote yes|no|abstain <proposal_id>
# Cron: */2 * * * *
#
# Requires: buzz-cli, bls-relayer, curl, python3, openssl;
#           /opt/dao-keys/dao-key.pem, $B/my-npub.hex, $B/my-juno1.txt,
#           $B/status-bot.sec.
# Committed state (get_vote) is verified before announcing — "broadcast OK"
# is never reported as a cast vote.
set -e
B=${B:-/opt/buzz/state}
CH=${GOV_CHANNEL:-2a6c9ad2-9da8-406c-8aa3-cd2f1e33af38}
NPUB_HEX=$(cat $B/my-npub.hex)
SEAT=$(cat $B/my-juno1.txt)
PM=$(cat $B/proposal_addr 2>/dev/null || echo juno1pt8gwn9ax4pvvlwm0s6nmxpgfvclfr9fmzr54620mqyclqa8k68q45x5wv)
SEEN=$B/vote-daemon-seen
GRPC=${GRPC:-127.0.0.1:9090}
API=${API:-http://127.0.0.1:8082}
touch $SEEN

MSGS=$(BUZZ_PRIVATE_KEY=$(sudo cat $B/status-bot.sec) BUZZ_RELAY_URL=wss://relay.junoclaw.xyz \
  /opt/buzz/bin/buzz-cli messages get --channel $CH --limit 50 2>/dev/null) || exit 0

echo "$MSGS" | python3 -c "
import json, sys, re
npub, seen_path = sys.argv[1], sys.argv[2]
seen = set(open(seen_path).read().split())
out, new_seen = [], set()
for m in json.load(sys.stdin):
    new_seen.add(m['id'])
    if m['id'] in seen or m.get('pubkey') != npub:
        continue
    mt = re.match(r'^\s*!vote\s+(yes|no|abstain)\s+(\d+)\s*$', m.get('content',''), re.I)
    if mt:
        out.append(f\"{mt.group(2)} {mt.group(1).lower()} {m['id']}\")
open(seen_path,'a').write(''.join(e+'\n' for e in new_seen))
print('\n'.join(out))
" "$NPUB_HEX" "$SEEN" > /tmp/.vote-intents.$$

post() {
  printf "%b" "$1" | sudo env BUZZ_PRIVATE_KEY="$(sudo cat $B/status-bot.sec)" \
    BUZZ_RELAY_URL=wss://relay.junoclaw.xyz \
    /opt/buzz/bin/buzz-cli messages send --channel $CH --content - >> $B/vote-daemon.log 2>&1
}

vote_state() {  # $1=proposal_id → prints existing ballot ("yes"/"no"/"abstain") or empty
  local VQ
  VQ=$(python3 -c "import base64,json;print(base64.urlsafe_b64encode(json.dumps({'get_vote':{'proposal_id':$1,'voter':'$SEAT'}}).encode()).decode().rstrip('='))")
  curl -fsS -m 10 "$API/api/wasm/smart/$PM?query=$VQ" 2>/dev/null | python3 -c \
    'import json,sys;d=json.load(sys.stdin).get("data",{});v=d.get("vote");print(v.get("vote") if v else "")' 2>/dev/null || true
}

while read -r PID VOTE EID; do
  [ -z "$PID" ] && continue
  EXIST=$(vote_state $PID)
  if [ -n "$EXIST" ]; then
    post "[dao:vote] seat ${NPUB_HEX:0:8}… already voted **$EXIST** on proposal #$PID — intent ignored (no revoting)"
    echo "$(date -Is) dup pid=$PID existing=$EXIST" >> $B/vote-daemon.log
    continue
  fi
  RES=$(JUNOCLAW_KEY_HEX="$(sudo openssl ec -in /opt/dao-keys/dao-key.pem -text -noout 2>/dev/null \
        | awk '/^priv:/{f=1;next}/^pub:/{f=0}f' | tr -d ' :\n')" \
        bls-relayer jc-execute --contract $PM \
        --msg "{\"vote\":{\"proposal_id\":$PID,\"vote\":\"$VOTE\"}}" \
        --layer-grpc $GRPC 2>&1)
  TXH=$(echo "$RES" | grep -oE '[0-9A-F]{64}' | head -1)
  # verify committed ballot before announcing — broadcast acceptance ≠ a vote
  BALLOT=""
  for i in 1 2 3 4 5 6; do
    sleep 5
    BALLOT=$(vote_state $PID)
    [ -n "$BALLOT" ] && break
  done
  if [ -n "$BALLOT" ]; then
    post "[dao:vote] seat ${NPUB_HEX:0:8}… cast **$BALLOT** on proposal #$PID — tx $TXH (committed on-chain)"
  else
    post "[dao:vote] seat ${NPUB_HEX:0:8}… intent $VOTE on #$PID FAILED to commit — tx ${TXH:-none} · $(echo "$RES" | tail -1 | head -c 160)"
  fi
  echo "$(date -Is) vote pid=$PID v=$VOTE tx=$TXH ballot=$BALLOT" >> $B/vote-daemon.log
done < /tmp/.vote-intents.$$
rm -f /tmp/.vote-intents.$$
exit 0
