#!/bin/bash
# Daily buzz relay backup: postgres dump + garage volume + configs.
set -e
B=/opt/buzz/backups
TS=$(date +%Y%m%d-%H%M)
mkdir -p $B

docker exec buzz-postgres pg_dump -U buzz buzz | gzip > $B/pg-$TS.sql.gz
docker run --rm -v buzz-relay_garage-data:/data:ro -v $B:/out   alpine tar czf /out/garage-$TS.tar.gz -C /data .
tar czf $B/config-$TS.tar.gz -C /   opt/buzz/compose.yaml opt/buzz/garage/garage.toml etc/caddy/Caddyfile   srv/www 2>/dev/null || true

find $B -name '*.gz' -mtime +14 -delete
du -sh $B; ls $B | tail -5
