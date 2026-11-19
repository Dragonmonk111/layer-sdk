# /opt/buzz daemons — production copies

Committed 2026-11-19 from OVH `/opt/buzz` (G0 bus-factor fix — these
previously lived only on the host; one wipe would have lost the whole
governance circuit).

| script | role | relay channel |
|---|---|---|
| status-bot.sh | seat/chain health feed | #agents |
| proposal-watch.sh | `[dao:proposal]` events | #governance |
| buzz-vote-daemon.sh | `!vote` → on-chain vote → `[dao:vote]` receipt | #governance |
| infra-monitor.sh | host/service checks | #agents |
| jc-alert-post.sh | jc-status JC_ALERT_CMD sink (stall/seat alerts) | #validator-ops |
| transcript-anchor.{sh,py} | window Merkle root → moultbook anchor | validator-ops |
| cw4-relay-watch.sh | legacy: cw4→allowlist sync (superseded by claw admission) | — |
| allowlist-add.sh | legacy Buzz psql allowlist | — |
| backup.sh | state/config backup | — |
| status-format.py, proposal-diff.py | helpers | — |
| soak-watchdog.sh | expired soak monitor (kept for reference) | — |

Keys are external by design: `$B/*.sec` files and
`/opt/dao-keys/dao-key.pem` live on the host only — never commit them.
Relay env default in these scripts is `ws://127.0.0.1:7070` (claw-relay);
public surface is `wss://relay.junoclaw.xyz`.
