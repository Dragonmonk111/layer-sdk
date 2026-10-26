#!/bin/bash
# jc-alert-post.sh — JC_ALERT_CMD sink for jc-status: reads one alert on
# stdin and posts it to #validator-ops (chain stall, seat absence, recovery).
# Wired via the jc-status systemd drop-in (Environment=JC_ALERT_CMD=...).
set -e
B=/opt/buzz/state
CH=810d3c45-acb5-44f8-9155-8bc94b3a12b3
{ printf 'alert | '; cat; } | BUZZ_PRIVATE_KEY=$(cat $B/status-bot.sec) \
  BUZZ_RELAY_URL=wss://relay.junoclaw.xyz \
  /opt/buzz/bin/buzz-cli messages send --channel $CH --content - >> $B/jc-alert.log 2>&1
