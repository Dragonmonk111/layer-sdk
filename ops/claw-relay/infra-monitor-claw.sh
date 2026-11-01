#!/bin/bash
# infra-monitor-claw.sh — post-cutover variant of ops/buzz/infra-monitor.sh.
# Checks every 15min, posts to #agents on state CHANGE (deduped), daily
# 06:00 summary. Buzz container checks replaced by claw-relay unit check.
set -e
B=/opt/buzz/state
CH=7b4f3765-8494-4de8-bf7a-b2595da827e0

post() {
  printf 'infra | %s' "$1" | BUZZ_PRIVATE_KEY=$(cat $B/status-bot.sec) \
    BUZZ_RELAY_URL=ws://127.0.0.1:7070 \
    /opt/buzz/bin/buzz-cli messages send --channel $CH --content -
}

ISSUES=""
DF=$(df / | awk 'NR==2{gsub(/%/,"",$5); print $5}')
AVAIL_MB=$(free -m | awk '/^Mem:/{print $7}')
[ "$DF" -ge 85 ] && ISSUES="$ISSUES disk=${DF}pct"
[ "$AVAIL_MB" -lt 1000 ] && ISSUES="$ISSUES mem_avail=${AVAIL_MB}MB"
systemctl is-active --quiet claw-relay || ISSUES="$ISSUES relay_inactive"
systemctl is-active --quiet junoclaw-g1 || ISSUES="$ISSUES node_inactive"
BKP=$(find /opt/claw/backups -name 'relay-*.redb' -mmin -1500 2>/dev/null | wc -l)
[ "$BKP" -eq 0 ] && ISSUES="$ISSUES backup_stale"

if [ -z "$ISSUES" ]; then STATE="ok"; else STATE="alert:$ISSUES"; fi
LAST=$(cat $B/last-infra-state 2>/dev/null || echo none)
if [ "$STATE" != "$LAST" ]; then
  if [ "$STATE" = "ok" ]; then
    post "recovered — disk=${DF}% mem_avail=${AVAIL_MB}MB relay=active node=active"
  else
    post "ALERT $ISSUES (disk=${DF}% mem_avail=${AVAIL_MB}MB)"
  fi
  echo "$STATE" > $B/last-infra-state
fi

if [ "$(date +%H)" = "06" ] && [ "$(cat $B/last-infra-summary 2>/dev/null)" != "$(date +%F)" ]; then
  post "daily: disk=${DF}% free=$(df -h / | awk 'NR==2{print $4}') mem_avail=${AVAIL_MB}MB relay=$(systemctl is-active claw-relay) node=$(systemctl is-active junoclaw-g1) data=$(du -sh /data 2>/dev/null | cut -f1) backups=$(ls /opt/claw/backups/relay-*.redb 2>/dev/null | wc -l) (14d-ret)"
  date +%F > $B/last-infra-summary
fi
