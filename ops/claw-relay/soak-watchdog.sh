#!/bin/bash
# soak-watchdog.sh — lives only during the 24h option-A soak. Checks
# claw-relay /healthz every 5min; on failure posts ONE alert to the BUZZ
# relay (#validator-ops) — daemon writes move to claw during the soak, so
# a claw outage must surface where operators still look. Dedupes via a
# state file so a dead relay alerts once, recovers once.
# Lifecycle: created for the soak window; REMOVE with the crontab line at
# soak end (revert or cutover), +24h expiry.
set -e
B=/opt/buzz/state
CH=810d3c45-acb5-44f8-9155-8bc94b3a12b3  # validator-ops

if curl -sf -m 5 http://127.0.0.1:7070/healthz >/dev/null; then
  STATE=ok
else
  STATE=down
fi
LAST=$(cat $B/last-soak-state 2>/dev/null || echo none)
if [ "$STATE" != "$LAST" ]; then
  if [ "$STATE" = "down" ]; then
    MSG="SOAK ALERT: claw-relay unhealthy — daemon writes are dropping. Revert: sed s|ws://127.0.0.1:7070|wss://relay.junoclaw.xyz|g on the six daemon scripts."
  else
    MSG="soak: claw-relay recovered — daemon writes landing again."
  fi
  printf '%s' "$MSG" | sudo env BUZZ_PRIVATE_KEY="$(sudo cat $B/status-bot.sec)" \
    BUZZ_RELAY_URL=wss://relay.junoclaw.xyz \
    /opt/buzz/bin/buzz-cli messages send --channel $CH --content - || true
  echo "$STATE" > $B/last-soak-state
fi
