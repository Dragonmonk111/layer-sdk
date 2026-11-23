
## 2026-11-23 — relay binary upgrade (Buzz NIP-11 identity)

Deployed claw-relay rebuilt from current source (sha256 `bbb522a9350679705cce10acfd59fd9ffdb46041fe33f74aae6f39c68d665f9f`, glibc ≤2.34).
Fix: plain GET now serves the NIP-11 identity document on ANY non-upgrade
GET (previously only `Accept: application/nostr+json`) — Buzz's probe
returned 426 → "relay did not advertise its identity". Verified live:
plain GET 200 with relay pubkey, WS 101 through the Caddy edge.
Backup: `/opt/claw/bin/claw-relay.bak-20261123` on OVH.
