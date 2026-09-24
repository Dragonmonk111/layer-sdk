# ICS-20 Demo — JunoClaw → Osmosis (local-osmosis)

Minimal-surface ICS-20 token transfer. JunoClaw is the **commitment writer**: it
stores ibc-go-encoded `ConnectionEnd`/`Channel`/packet-commitment bytes at
`ibc/<ics24_path>` keys in its `state_root`-committed KV store. Osmosis runs real
ibc-go; its **08-wasm BLS light client** verifies JunoClaw's commitments via
`verify_membership` (Merkle proof over the BLS-signed `state_root`).

## Trust model (devnet)

- **Osmosis → JunoClaw:** genuine. The 08-wasm contract verifies each proof
  against the BLS threshold certificate — this is the real thing being demoed.
- **JunoClaw → Osmosis:** nominal. JunoClaw does not run a Tendermint client for
  Osmosis; the `*OpenAck` steps advance INIT→OPEN on relayer instruction and
  carry the counterparty proof opaquely. Production (A56) adds the real verifier.

## Identifiers

| Symbol        | Meaning                                        | Example          |
| ------------- | ---------------------------------------------- | ---------------- |
| `JC_CLIENT`   | JunoClaw's client for Osmosis                  | `07-tendermint-0`|
| `WASM_CLIENT` | Osmosis's 08-wasm client for JunoClaw          | `08-wasm-5`      |
| `JC_CONN`     | JunoClaw's connection id                       | `connection-0`   |
| `OSMO_CONN`   | Osmosis's connection id (assigned by conn-try) | `connection-2`   |
| `JC_CHAN`     | JunoClaw's channel id                          | `channel-0`      |
| `OSMO_CHAN`   | Osmosis's channel id (assigned by chan-try)    | `channel-2`      |

> `JC_CLIENT` must be a valid ibc-go identifier (9–64 chars). `client-0` is too
> short and is rejected — use `07-tendermint-0`.

## Flags

- `--layer-grpc` JunoClaw gRPC (default `127.0.0.1:9090`)
- `--grpc` Osmosis gRPC (default `127.0.0.1:9190`)
- `--key-hex` / `RELAYER_KEY_HEX` — funded **Osmosis** secp256k1 key
- `--bech32-prefix osmo --fee-denom uosmo --cp-chain-id <osmo-chain-id>`
- JunoClaw txs sign with the deployer key (`SHA256("junoclaw-deployer-v1")`);
  override with `--jc-key-hex` / `JUNOCLAW_KEY_HEX`.

## Timing

Every Osmosis proof step needs a consensus state at `proof_height =
state_height + 1`. The relayer's proof commands (`conn-try`, `conn-confirm`,
`chan-try`, `chan-confirm`, `recv-packet`) **auto-update the 08-wasm client to
`proof_height` before submitting** — no separate `update-client` step is needed.
They therefore require `--client-id <WASM_CLIENT>` and print the `proof_height`
they used. Just leave ~1 block after each JunoClaw tx so the commitment is
committed before the proof is assembled.

## Sequence

```bash
# 0. Register JunoClaw's nominal client for Osmosis.
bls-relayer jc-create-client --client-id 07-tendermint-0 --cp-chain-id <osmo-chain-id> --height 1

# --- Connection handshake ---
# 1. JunoClaw: conn-open-init  (writes ConnectionEnd{INIT} at ibc/connections/connection-0)
bls-relayer jc-conn-init --client-id 07-tendermint-0 --connection-id connection-0 --cp-client-id 08-wasm-5

# 2. Osmosis: conn-open-try (auto-updates client, verifies JunoClaw conn INIT). Note OSMO_CONN from events.
bls-relayer conn-try --client-id 08-wasm-5 --cp-client-id 07-tendermint-0 --cp-connection-id connection-0 \
    --grpc <osmo-grpc> --key-hex $K --bech32-prefix osmo --fee-denom uosmo --cp-chain-id <osmo-chain-id>

# 3. JunoClaw: conn-open-ack (INIT→OPEN).
bls-relayer jc-conn-ack --connection-id connection-0 --cp-connection-id <OSMO_CONN>

# 4. Osmosis: conn-open-confirm (auto-updates client, verifies JunoClaw conn OPEN).
bls-relayer conn-confirm --client-id 08-wasm-5 --connection-id <OSMO_CONN> --cp-connection-id connection-0 \
    --grpc <osmo-grpc> --key-hex $K --bech32-prefix osmo --fee-denom uosmo --cp-chain-id <osmo-chain-id>

# --- Channel handshake ---
# 5. JunoClaw: chan-open-init (writes Channel{INIT} at ibc/channelEnds/ports/transfer/channels/channel-0)
bls-relayer jc-chan-init --channel-id channel-0 --connection-id connection-0

# 6. Osmosis: chan-open-try (auto-updates client, verifies chan INIT). Note OSMO_CHAN.
bls-relayer chan-try --client-id 08-wasm-5 --connection-id <OSMO_CONN> --cp-channel-id channel-0 --cp-port-id transfer \
    --grpc <osmo-grpc> --key-hex $K --bech32-prefix osmo --fee-denom uosmo --cp-chain-id <osmo-chain-id>

# 7. JunoClaw: chan-open-ack (INIT→OPEN).
bls-relayer jc-chan-ack --channel-id channel-0 --cp-channel-id <OSMO_CHAN>

# 8. Osmosis: chan-open-confirm (auto-updates client, verifies chan OPEN).
bls-relayer chan-confirm --client-id 08-wasm-5 --channel-id <OSMO_CHAN> --cp-channel-id channel-0 --cp-port-id transfer \
    --grpc <osmo-grpc> --key-hex $K --bech32-prefix osmo --fee-denom uosmo --cp-chain-id <osmo-chain-id>

# --- ICS-20 transfer ---
# 9. JunoClaw: escrow + packet commitment at ibc/commitments/ports/transfer/channels/channel-0/sequences/1
#    IMPORTANT: use --timeout-height 0 (timestamp-only). A non-zero rev=0 timeout
#    height reads as already-elapsed against Osmosis's rev=1 selfHeight.
bls-relayer jc-transfer --channel-id channel-0 --amount 1000ujclaw --to <osmo_receiver> \
    --timeout-height 0 --timeout-timestamp <future_ns>

# 10. Osmosis: recv-packet (auto-updates client, verifies packet commitment → mints voucher).
#     --timeout-height/--timeout-timestamp must match the transfer exactly.
bls-relayer recv-packet --client-id 08-wasm-5 --channel-id <OSMO_CHAN> --cp-channel-id channel-0 --cp-port-id transfer \
    --sequence 1 --amount 1000ujclaw --to <osmo_receiver> --timeout-height 0 --timeout-timestamp <future_ns> \
    --grpc <osmo-grpc> --key-hex $K --bech32-prefix osmo --fee-denom uosmo --cp-chain-id <osmo-chain-id>

# 11. (optional) JunoClaw: clear the packet commitment on ack.
bls-relayer jc-ack --channel-id channel-0 --sequence 1
```

## Wire-format notes

- **JunoClaw txs** carry the `IbcMsg` JSON-encoded in a single `Any` under
  `/junoclaw.ibc.v1.Msg`, signed `junoclaw-1` / account `17` / `ujclaw`.
- **Osmosis txs** are real ibc-go protos. `conn-open-try`'s `client_state` /
  `proof_client` / `proof_consensus` are deprecated in ibc-go v8+ and left empty.
- The counterparty `MerklePrefix` is `"ibc/"`, so ibc-go builds
  `key_path = ["ibc/", "<ics24_path>"]` and the contract's
  `concat(key_path) = "ibc/<ics24_path>"` matches JunoClaw's storage key.
