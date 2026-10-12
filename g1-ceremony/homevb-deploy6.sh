#!/usr/bin/env bash
# rc6 deploy for home-vb — run inside WSL Ubuntu-24.04 as root.
# Swaps /opt/g1-node/slay3rd, restarts via systemd-run (setsid children die
# with the invoking wsl session), keeps file-logging parity via append:.
set -e
NEW=/root/junoclaw-chain/target/release/slay3rd
DST=/opt/g1-node/slay3rd
LOG=/opt/g1-node/slay3rd.log

sha256sum "$NEW"
cp "$DST" "$DST.rc5.bak"
cp "$NEW" "$DST.rc6"
install -m 755 "$NEW" "$DST"

# stop the running node (pid discovered live — it changes)
OLD=$(pgrep -f '^\./slay3rd /config/node.toml$' || pgrep -f '/opt/g1-node/slay3rd' || true)
if [ -n "$OLD" ]; then kill "$OLD"; sleep 3; kill -9 "$OLD" 2>/dev/null || true; fi

# relaunch as a transient systemd unit with file-append logging
systemd-run --unit=g1-node --collect \
  --property=WorkingDirectory=/opt/g1-node \
  --property=StandardOutput=append:$LOG \
  --property=StandardError=append:$LOG \
  --property=Restart=no \
  $DST /config/node.toml
sleep 8
systemctl is-active g1-node
tail -12 $LOG
echo DEPLOY_DONE
