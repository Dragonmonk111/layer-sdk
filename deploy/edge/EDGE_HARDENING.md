# Edge hardening — OVH public surface

Status: **DEPLOYED 2026-10-14** on OVH (51.195.201.74, `ubuntu@` + `~/.ssh/ovh_g1`).

## Deployed exposure map

| Port | Status | Owner | Notes |
|---|---|---|---|
| 80/443 | open | Caddy TLS edge | per-IP rate limits (below) at both layers |
| 7001 | open | consensus P2P (Ed25519-auth) | kept un-throttled — see §3 |
| 8080 | **closed** | — | stale G1 UFW rules deleted |
| 9090 | **scoped** | node gRPC | `ALLOW IN 84.9.189.220` only (monitor/archiver IP) |
| 3000/26658 | closed | — | — |

## 1. UFW (deployed)

Final rules — `ufw status numbered`:

```
22/tcp    ALLOW  Anywhere          # SSH
7001/tcp  ALLOW  Anywhere          # consensus P2P
9090/tcp  ALLOW  84.9.189.220      # g1 monitor / VM archiver
80,443    ALLOW  Anywhere          # public edge (+v6 duplicates)
```

What changed: `8080/tcp Anywhere` deleted (v4+v6), `9090/tcp Anywhere`
("g1 statesync donor (temp)") deleted (v4+v6). The VM archiver's existing
connection verified intact after the cut (`ss -tnp` showed the ESTABLISHED
session to 84.9.189.220 and Caddy's localhost proxy both alive).

## 2. iptables edge limits (deployed, persistent)

Per-IP limits applied to NEW connections on the public edge, added to
`/etc/ufw/before.rules` so they survive `ufw reload` and reboot:

```
-A ufw-before-input -p tcp --dport 80  -m conntrack --ctstate NEW -m hashlimit --hashlimit-above 15/sec --hashlimit-burst 40 --hashlimit-mode srcip --hashlimit-name rl80  -j DROP
-A ufw-before-input -p tcp --dport 80  -m connlimit --connlimit-above 25 --connlimit-mask 32 -j DROP
-A ufw-before-input -p tcp --dport 443 -m conntrack --ctstate NEW -m hashlimit --hashlimit-above 15/sec --hashlimit-burst 40 --hashlimit-mode srcip --hashlimit-name rl443 -j DROP
-A ufw-before-input -p tcp --dport 443 -m connlimit --connlimit-above 25 --connlimit-mask 32 -j DROP
```

Effect: >15 new TCP conns/s per IP (after a 40-conn burst) or >25
concurrent connections per IP get dropped before they reach Caddy.
Verified: survives `ufw reload`; endpoints returned 200 throughout.

## 3. Caddy rate limiting (deployed)

Stock `caddy` (v2.11.7) has no `rate_limit` handler. Deployed a custom
xcaddy build:

```bash
# on the host (go 1.23.4 bootstrap in /tmp/go123)
xcaddy build v2.11.7 --with github.com/mholt/caddy-ratelimit --output /tmp/caddy-rl
setcap 'cap_net_bind_service=+ep' /usr/bin/caddy   # after swap
systemctl restart caddy
```

Per-IP sliding-window zones in `/etc/caddy/Caddyfile` (mirrored in this
repo's `./Caddyfile`):

| Site | Zone | Limit |
|---|---|---|
| api.junoclaw.xyz | api_per_ip | 60 events/min |
| explorer.junoclaw.xyz | explorer_per_ip | 120 events/min |
| status.junoclaw.xyz | status_per_ip | 120 events/min |

Verified: 130-request loop against status.junoclaw.xyz → 200s then clean
429s past the zone limit.

Host backups: `/etc/caddy/Caddyfile.bak-jc`, `/usr/bin/caddy.stock-bak`,
`/etc/ufw/before.rules.bak-jc`.

## 4. Port 7001 — intentionally un-throttled

Consensus P2P must accept validator peers freely; it's already
Ed25519-authenticated and rate-limiting new connections could stall a
restarting seat mid-window. Left open at every layer.

## 5. Rollback

```bash
# caddy
sudo cp /etc/caddy/Caddyfile.bak-jc /etc/caddy/Caddyfile
sudo cp /usr/bin/caddy.stock-bak /usr/bin/caddy
sudo systemctl restart caddy

# firewall edge limits (removes only the junoclaw block)
sudo cp /etc/ufw/before.rules.bak-jc /etc/ufw/before.rules
sudo ufw reload
```

## 6. Faucet route (when deployed)

`jc-faucet` lands at `faucet.junoclaw.xyz` — add a site block proxying to
its listen port with a ~30 events/min `faucet_per_ip` zone. The service
enforces its own per-address/per-IP/daily caps; the edge is the outer cap.
