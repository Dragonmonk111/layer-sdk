# Edge hardening — OVH public surface

Runbook for the TLS edge on the OVH host (51.195.201.74). Verified exposure
map (2026-10-14):

| Port | Status | Owner | Action |
|---|---|---|---|
| 80/443 | open | Caddy TLS edge | keep; add rate limits (below) |
| 7001 | open | consensus P2P (authenticated) | keep — required for G2 peers |
| 8080 | closed | — | already done |
| 9090 | open | node gRPC | keep open BUT connection-limit; still used by the VM archiver |
| 3000/26658 | closed | — | good |

## 1. Caddy rate limits

Stock `caddy:2-alpine` has no rate limiting — needs the
`mholt/caddy-ratelimit` plugin, so build the local image:

```bash
cd deploy/edge
docker build -f Dockerfile.caddy -t junoclaw-caddy:ratelimit .
```

Then on the host:

1. `cat /etc/caddy/Caddyfile` (or wherever the live config is) and merge the
   `rate_limit { zone … }` blocks from `./Caddyfile` into each existing site
   block — the upstream addresses/ports there are authoritative, this file's
   are placeholders for the known services.
2. Swap the caddy container image to `junoclaw-caddy:ratelimit` in its
   compose file and `docker compose up -d caddy` (or `caddy reload` if running
   on the host).
3. Verify: `for i in $(seq 1 70); do curl -s -o /dev/null -w '%{http_code}\n' https://status.junoclaw.xyz/; done | sort | uniq -c`
   — expect 200s then 429s past the limit, and a `Retry-After` header.

## 2. Port 9090 — keep but limit

The VM archiver holds a single long-lived gRPC connection, so new-connection
rate limiting is safe and blunts flood/scans:

```bash
ufw limit 9090/tcp comment 'grpc - archiver + queries; new-conn limited'
ufw status numbered | grep 9090
```

If a future change makes the archiver tunnel out (e.g. over the edge or a
VPN), 9090 can drop to localhost-only:

```bash
# later, when nothing remote needs it:
ufw delete limit 9090/tcp
ufw allow from 127.0.0.1 to any port 9090 proto tcp
```

## 3. Do NOT limit 7001

Consensus P2P must accept validator peers freely; it is already
Ed25519-authenticated and rate limiting new connections could stall a
restarting seat. Leave it open and un-throttled.

## 4. Faucet edge route

When `jc-faucet` deploys (tools/jc-faucet), the `faucet.junoclaw.xyz` site
block in ./Caddyfile routes it. The service itself enforces per-address,
per-IP, and daily caps — the edge zone is a coarse outer cap only.
