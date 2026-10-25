#!/bin/bash
# proposal-watch.sh — mirror DAO proposal lifecycle into #governance.
# Polls the proposal module via the local API, posts diffs signed by the
# gov-bot identity. Cron: */2 * * * *
set -e
B=/opt/buzz/state
CH=2a6c9ad2-9da8-406c-8aa3-cd2f1e33af38
PM=$(cat $B/proposal_addr 2>/dev/null || echo juno1pt8gwn9ax4pvvlwm0s6nmxpgfvclfr9fmzr54620mqyclqa8k68q45x5wv)
Q=$(python3 -c 'import base64;print(base64.urlsafe_b64encode(b"{\"list_proposals\":{}}").decode().rstrip("="))')
RESP=$(curl -fsS -m 10 "http://127.0.0.1:8082/api/wasm/smart/$PM?query=$Q" 2>/dev/null) || exit 0
while IFS= read -r line; do
  [ -z "$line" ] && continue
  printf "%b" "$line" | sudo env BUZZ_PRIVATE_KEY="$(sudo cat $B/status-bot.sec)" BUZZ_RELAY_URL=wss://relay.junoclaw.xyz /opt/buzz/bin/buzz-cli messages send --channel $CH --content - >> $B/proposal-watch.log 2>&1 \
    && echo "$(date -Is) posted" >> $B/proposal-watch.log
done < <(echo "$RESP" | python3 /opt/buzz/proposal-diff.py $B/proposals-seen.json)
exit 0
