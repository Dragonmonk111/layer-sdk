#!/bin/bash
# status-bot.sh — post chain status to #agents: heartbeat every 30min,
# immediate on state change (degraded/recovered).
set -e
B=/opt/buzz/state
CH=7b4f3765-8494-4de8-bf7a-b2595da827e0
J=$(curl -fsS -m 10 http://127.0.0.1:8082/api/status) || exit 1
OUT=$(echo "$J" | python3 /opt/buzz/status-format.py)
MSG=$(echo "$OUT" | head -1)
STATE=$(echo "$OUT" | tail -1)
LAST=$(cat $B/last-bot-state 2>/dev/null || echo none)
M=$(date +%M)
if [ "$STATE" != "$LAST" ] || [ "$M" -lt 30 ]; then
  echo "status-bot | $MSG" | BUZZ_PRIVATE_KEY=$(cat $B/status-bot.sec)     BUZZ_RELAY_URL=wss://relay.junoclaw.xyz     /opt/buzz/bin/buzz-cli messages send --channel $CH --content -     && echo "$STATE" > $B/last-bot-state
fi
