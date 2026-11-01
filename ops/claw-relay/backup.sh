#!/bin/bash
# claw-relay daily backup — replaces ops/buzz/backup.sh post-cutover.
# Surface: relay.redb (single file — far simpler than pg_dump), unit
# file, allowlist + state. 14-day retention, same cadence as buzz's.
set -e
B=/opt/claw/backups
TS=$(date +%Y%m%d-%H%M)
mkdir -p $B

# redb is crash-consistent by design — a copy while running is a valid
# snapshot; fsync'd commits are never torn (worse case: newest tail rows
# absent). Still take it quiesced-adjacent: copy via cp --reflink=auto.
cp --reflink=auto /opt/claw/db/relay.redb $B/relay-$TS.redb

tar czf $B/config-$TS.tar.gz -C / \
  etc/systemd/system/claw-relay.service \
  opt/claw/state/allowlist.txt \
  srv/www 2>/dev/null || true

find $B -name '*.redb' -o -name '*.gz' | xargs -r ls -t | tail -n +15 | xargs -r rm -f
find $B -name 'relay-*.redb' -mtime +14 -delete
find $B -name 'config-*.tar.gz' -mtime +14 -delete
du -sh $B; ls $B | tail -5
