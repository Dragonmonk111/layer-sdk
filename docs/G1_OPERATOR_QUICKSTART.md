# G1 Operator Quickstart — JunoClaw `junoclaw-g1`

*For invited operators (Ravi and friends). 10 minutes of ceremony, then the
node runs itself.*

## What you need

- A Linux machine with a **stable public IP** and **inbound TCP 7001** open.
  Cloud VM, VPS, or home node behind port-forward all work. 2 cores / 4 GB /
  20 GB minimum (4c/8G/50G recommended).
- Docker (or the `slay3rd` Linux binary directly). gRPC 9090 can stay closed.
- Telegram/Signal reachability for the coordinated launch window.
- NTP sync (`timedatectl` should show `synchronized: yes`).

## Step 1 — Get the release

```bash
# from the coordinator (or the release page)
sha256sum -c SHA256SUMS.txt
docker load < junoclaw-chain-v0.6.0-rc1.tar.gz
chmod +x generate-testnet-keys-linux-x86_64
mv generate-testnet-keys-linux-x86_64 generate-testnet-keys
```

## Step 2 — Generate your identity (keys never leave your machine)

```bash
./generate-testnet-keys keygen-share \
    --output-dir ./my-validator --name <your-moniker> --p2p <your-public-ip>:7001
```

Output:

- `my-validator/keys.json` — **PRIVATE. Never send this to anyone.**
- `my-validator/share-request.json` — public material only. **Send this
  file to the coordinator** (any channel — email, DM, GitHub gist).

That's all you do until the ceremony.

## Step 3 — Receive your package

The coordinator runs `assemble-genesis` with all share-requests and sends
you back, over an **encrypted channel** (age / encrypted DM):

- `bls-share.json` — your private BLS share
- `node-<i>.toml` — your config (peers already wired, `chain_id = "junoclaw-g1"`)
- `genesis.json` — identical for every validator

## Step 4 — Finalize and boot

```bash
./generate-testnet-keys finalize \
    --keys ./my-validator/keys.json --bls-share ./bls-share.json
# verifies your share against the polynomial + your MAYO2 identity
rm bls-share.json   # its contents now live in keys.json

mkdir -p data config keys
cp my-validator/keys.json keys/keys.json
cp genesis.json config/genesis.json
cp node-*.toml config/node.toml

docker run -d --name junoclaw-g1 --restart unless-stopped \
  -p 7001:7001 \
  -v "$PWD/keys:/keys:ro" -v "$PWD/config:/config:ro" -v "$PWD/data:/data" \
  junoclaw-chain:v0.6.0-rc1
```

## Step 5 — Confirm

```bash
docker logs -f junoclaw-g1 | grep CONS-05
# "Block finalized with BLS threshold certificate (CONS-05) height=N ..."
# heights should match the other validators within a few seconds
```

Your `validator_index` is in the finalized `keys.json` — sanity-check it
matches your `node.toml`.

## Failure drills (do these once after launch)

- **Kill the leader**: `docker stop <current-leader>` on whichever node is
  leading; chain skips that view and finalizes within ~3 s (leader_timeout).
- **Restart**: `docker restart junoclaw-g1` — catches up via backfill.

## Do NOT

- Do not send `keys.json` or `bls-share.json` to anyone — coordinator
  included. Only `share-request.json` is public.
- Do not set `insecure_devnet` — the node will refuse the real genesis
  with it, and it's meant to.
- Do not run two nodes with the same `keys.json`.
