#!/usr/bin/env bash
# ovh-sentinel-remote.sh - read-only battery executed on ovh-uk1
# via stdin (ssh ... bash -s) by ovh-sentinel.ps1. Keep the =KEY
# value output format; the PS side parses on those prefixes.
echo "=SVC $(systemctl is-active junoclaw-g1)"
echo "=LOAD $(cut -d' ' -f1 /proc/loadavg)"
echo "=MEM $(free -m | awk '/^Mem:/{print $2"/"$3"/"$7}')"
echo "=DISK $(df -h / | awk 'NR==2{print $3"/"$5"/"$4}')"
P=$(pgrep -x slay3rd | head -1)
echo "=RSS $(grep VmRSS /proc/$P/status | awk '{print $2}')"
echo "=HWM $(grep VmHWM /proc/$P/status | awk '{print $2}')"
echo "=SYNC $(timedatectl | grep -c 'System clock synchronized: yes')"
echo "=ERR $(sudo -n journalctl -u junoclaw-g1 --since '30 min ago' --no-pager 2>/dev/null | grep -c ' ERROR ')"
