# JunoClaw G2 — Operator Pack (Anatoly / new seats)

Everything below runs on YOUR box. Private keys never leave it — the only
file that travels is `share-request.json` (public material).

## 1. Install the ceremony tool

```bash
cd /tmp
curl -fsSL -o generate-testnet-keys \
  https://github.com/Dragonmonk111/layer-sdk/releases/download/v0.6.0-rc10/generate-testnet-keys-linux-amd64
echo "75331239e22449b12244784830d04a0645ff1d5efce49678352aa7bb8e3ddcf9  generate-testnet-keys" | sha256sum -c -
chmod +x generate-testnet-keys
sudo mv generate-testnet-keys /usr/local/bin/
```

(sha256 pins the exact binary we built from commit `3ad6a23` — the
release manifest covers slay3rd; this hash is the tool's pin.)

## 2. Install the node binary

```bash
curl -fsSL https://github.com/Dragonmonk111/layer-sdk/releases/download/v0.6.0-rc10/install.sh | sh
# installs /usr/local/bin/slay3rd, checksum-verified against MANIFEST.sha256
```

## 3. Generate your keys + share request

```bash
mkdir -p ~/junoclaw-g2 && cd ~/junoclaw-g2
generate-testnet-keys keygen-share \
  --output-dir . \
  --name anatoly-hetzner \
  --p2p <YOUR-PUBLIC-IP>:7001
```

This creates:
- `keys.json` — **PRIVATE.** Ed25519 + MAYO2 + (later) your BLS share.
  Never send it anywhere. Back it up somewhere safe.
- `share-request.json` — **public.** Send this to us (Telegram DM or the
  ops channel). Contains your pubkeys, moniker, p2p address, and an
  attestation proving you own the key.

## 3.5 Identity bundle — governance + comms (one-time, ~2 min)

Your validator seat comes with DAO voting rights and relay access. Two
more keys — deliberately separate from your node keys:

**Governance cold key** (signs DAO votes — guard like `keys.json`):

```bash
openssl ecparam -name secp256k1 -genkey -noout -out dao-key.pem
openssl ec -in dao-key.pem -pubout -outform DER | tail -c 33 | xxd -p -c 33
```

Send the printed pubkey hex with your `share-request.json` — we derive
your `juno1` member address from it. `dao-key.pem` never leaves the box.

**Chat hot key** (your npub on relay.junoclaw.xyz — rotatable, low-risk):

```bash
# any Nostr tool, e.g.:
nak key generate        # keep the nsec private, send us the npub
```

Send the `npub` too. Your DAO onboarding proposal will carry both:
**one ratification admits your seat, your cw4 vote, and your relay
identity** — no separate key collection rounds later.

## 4. Firewall / ports

- `7001/tcp` — P2P (must be reachable from the other validators)
- `9090/tcp` — gRPC (optional; only if you want to serve public queries —
  say so and we'll add you to the RPC pool)

SSH key-only + UFW + Hetzner firewall — your setup is already right.

## 5. What happens next (the ceremony, ~15 min of your time, async)

1. We run `dkg-info` for round 1 (reshare: preserves the group key and
   the Osmosis IBC trust root) and send you `dkg-info.json` plus one
   `dealing-priv-*-for-you.json` file per dealer.
2. You run:
   ```bash
   generate-testnet-keys dkg-ack --info dkg-info.json --keys keys.json \
     --priv-dir ./priv --out-dir ./acks
   ```
   and send back the `acks/*.json` files.
3. After all logs are in, you run `dkg-finalize` + `finalize` — your
   BLS share materializes **locally**. No secret share ever crosses
   the wire.
4. We send you `node.toml` (peer table + `restart_anchor`). On window
   day (coordinated 30 min): park `data/consensus`, swap configs,
   `systemctl start junoclaw`. Chain resumes ~1 min after quorum joins.

## 6. During the window

Be online on Telegram at the agreed UTC time. The halt affects the live
chain — it resumes as soon as 3-of-4 seats are up. Total expected
downtime: ~15-30 min. Your first finalized block with your signature in
the cert is the "you're in" moment.

## Requirements recap

- Ubuntu 24.04 ✔ | 4 vCPU / 8 GB / 75 GB ✔ (min: 2/4/20)
- NTP deviation < 2 s ✔ (you're at ~0.1 ms)
- Key-only SSH + firewall ✔
- Uptime: this is a live BFT seat — flag maintenance windows in advance
