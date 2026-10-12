#!/bin/bash
# probe-height.sh - current chain height + recent finalize cadence from home-vb log
LOG=/opt/g1-node/slay3rd.log
echo "=== last height mentions ==="
tail -2000 "$LOG" | grep -oE 'height[^0-9]{0,5}[0-9]+' | tail -8
echo "=== last finalize/block lines ==="
tail -2000 "$LOG" | grep -iE 'finaliz|executed|committed' | tail -8
