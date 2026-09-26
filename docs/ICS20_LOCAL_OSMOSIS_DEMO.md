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

## Relay daemon (unattended)

Steps 10–11 above are the manual path. The `relay` subcommand runs the same
logic as a loop so the bridge operates without babysitting:

```bash
bls-relayer relay \
  --client-id 08-wasm-5 \
  --channel-id channel-0 --cp-channel-id channel-2 \
  --port-id transfer --cp-port-id transfer \
  --interval 6 --update-cadence 50 \
  --max-retries 60 --min-fee-balance 0 \
  --fee-amount 20000 --health-addr 127.0.0.1:18080
```

Each tick it:

1. **Fee preflight** — queries the counterparty `cosmos.bank.v1beta1` balance of
   the relayer key and pauses the tick (with a PAUSED log) if it can't cover one
   tx fee (`--min-fee-balance`; `0` = auto = `--fee-amount`). The JunoClaw side
   gets the same check best-effort (warns only — its gRPC may not expose bank).
2. Reads `ibc/nextSequenceSend/ports/transfer/channels/<JC_CHAN>` via the
   lightclient `Proof` query's `value`, and the counterparty tip
   (`GetLatestBlock`) for timeout detection.
3. For every sequence from the scan floor whose `ibc/commitments/...` key is
   still present, resolves it in order:
   - **acked on the counterparty** (`Query/PacketAcknowledgement`) → `jc-ack`
     to clear JunoClaw's commitment;
   - **timeout elapsed** (packet `timeout_height`/`timeout_timestamp` vs cp tip)
     → `IbcMsg::Timeout` on JunoClaw: the keeper refunds the escrowed tokens to
     the *original sender* (read from the stored packet, never the message) and
     clears commitment + packet data;
   - **otherwise** → reads the stored packet at `ibc/packetData/...`, proves the
     commitment, auto-updates the client, submits `MsgRecvPacket`, then `jc-ack`.
     A sequence that keeps failing is skipped after `--max-retries` attempts
     (`0` = unlimited) and reported once — resolve it manually (`jc-ack` or let
     it time out).
4. **Keepalive**: advances the 08-wasm client every `--update-cadence` blocks.

The scan floor advances to the lowest still-pending sequence each tick, so a
healthy daemon does O(1) storage reads per tick instead of rescanning history.

Flag convention is **JunoClaw-centric** (opposite of `recv-packet`):
`--channel-id` is the JunoClaw *source* channel, `--cp-channel-id` the
counterparty *dest* channel.

**Robustness** — the tick loop is hardened for unattended operation:

- each tick runs inside `catch_unwind`; a panic is counted and logged, never
  kills the daemon;
- consecutive tick errors back off exponentially (interval ×2ⁿ, capped at 60s);
- a `heartbeat` line logs ~once a minute with tick/packet/error counters;
- `--health-addr` serves stats JSON on every request — wire it to your monitor
  and alert on `last_tick_unix` staleness, `errors`/`panics` growth, or
  `packets_pending` accumulation.

**Operational notes** — learned from the live e2e run:

- **Counterparty fee**: `--fee-amount` must satisfy the counterparty's min
  gas price. Local osmosis rejects `5000uosmo` (`code 13: insufficient fees;
  required: 10000uosmo`) — run `--fee-amount 20000` for headroom. A failed
  recv burns a retry attempt, so a too-low fee silently exhausts
  `--max-retries` and skips the packet.
- **Health port**: pick a port that is actually free — `8080` is bound by
  `AgentService` on this Windows host (bind failure only warns and disables
  the endpoint; the relay loop is unaffected).
- **Ordering**: `channel-0` is `UNORDERED`, so a packet that exhausts
  `--max-retries` is skipped without stalling later sequences. On an
  `ORDERED` channel the same skip would wedge every subsequent packet —
  treat `packets_pending` growth on ordered channels as page-worthy.
- **Fee payer**: daemon-side `IbcMsg::Timeout`/`Acknowledgement` txs ignore
  `--to`/`--from`; the JunoClaw fee payer and signer is always `--key-hex`.
  `--key-hex` on a command line is devnet practice — in production load it
  from a systemd `EnvironmentFile` (mode 0600) or a keyring.

> Requires the keeper change that stores the full packet at `ibc/packetData/...`
> (the commitment path only holds `sha256(packet)`, which is not reversible).
> Deploy via the state-preserving binary swap; packets sent before the upgrade
> have no stored `packetData` and are skipped (relay them once via the manual
> `recv-packet` path). Timeout refund requires the `IbcMsg::Timeout` keeper
> handler — deploy both in the same swap.

### Supervision

The daemon never exits on its own; pair it with a restart policy so a host or
process-level failure recovers automatically.

**Docker** — `docker-compose.yml` service:

```yaml
relayer:
  image: junoclaw-bls-relayer:latest
  restart: unless-stopped
  command: >
    relay --client-id 08-wasm-5
    --channel-id channel-0 --cp-channel-id channel-2
    --interval 6 --update-cadence 50
    --health-addr 0.0.0.0:8080
  ports: ["127.0.0.1:8080:8080"]
```

**systemd** — `/etc/systemd/system/junoclaw-relayer.service`:

```ini
[Unit]
Description=JunoClaw IBC relay daemon
After=network.target

[Service]
ExecStart=/usr/local/bin/bls-relayer relay \
  --client-id 08-wasm-5 --channel-id channel-0 --cp-channel-id channel-2 \
  --interval 6 --update-cadence 50 --health-addr 127.0.0.1:8080
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

The in-process `catch_unwind` + backoff already handles per-tick failures; the
restart policy is the backstop for process exit (OOM, unrecoverable runtime
state). Keep `RestartSec` small — on restart the daemon re-derives all state
from on-chain commitments, so a crash mid-tick just re-runs idempotently.

## Wire-format notes

- **JunoClaw txs** carry the `IbcMsg` JSON-encoded in a single `Any` under
  `/junoclaw.ibc.v1.Msg`, signed `junoclaw-1` / account `17` / `ujclaw`.
- **Osmosis txs** are real ibc-go protos. `conn-open-try`'s `client_state` /
  `proof_client` / `proof_consensus` are deprecated in ibc-go v8+ and left empty.
- The counterparty `MerklePrefix` is `"ibc/"`, so ibc-go builds
  `key_path = ["ibc/", "<ics24_path>"]` and the contract's
  `concat(key_path) = "ibc/<ics24_path>"` matches JunoClaw's storage key.

## JunoClaw CosmWasm lifecycle (native, no gov wrapping)

The chain decodes standard wasmd `/cosmwasm.wasm.v1.*` `Any`s into `WasmMsg`
(`packages/cosmos/src/msg.rs`), so the same `jc-*` signing path drives a full
contract lifecycle directly — no IBC or governance hop:

```sh
# Store — prints wasm size + sha256 checksum; code id is the next free counter.
bls-relayer jc-store-code --wasm path/to/contract.wasm

# Instantiate — resolves + prints the new contract address via
# Query/ContractsByCode (polls ~30s for inclusion).
bls-relayer jc-instantiate --code-id <n> [--label <l>] [--msg <json|@file>]

# Execute — MsgExecuteContract; --msg @file reads JSON from disk (needed for
# MAYO vectors, whose Vec<u8> pk/sig fields are multi-KB int arrays).
bls-relayer jc-execute --contract <addr> --msg <json|@file>

# Queries — abci_query, no gas, no sequence.
bls-relayer jc-query     --contract <addr> --msg <json|@file>
bls-relayer jc-contracts --code-id <n>
```

All sign with the deployer key (`sha256("junoclaw-deployer-v1")` → `juno1dz875zg8p78anpjv3f0qt4gu5a3awpjfhtw992`);
`--jc-key-hex` / `JUNOCLAW_KEY_HEX` overrides. `--simulate` dry-runs via
`tx.v1beta1.Service/Simulate` — currently `Unimplemented` on the chain.

### Devnet record — jclaw-credential (MAYO-2), 2026-09-26

- **code_id** `2` — `junoclaw/devnet/artifacts/jclaw_credential_mvp.wasm`,
  313 418 B, checksum `b26230fd31f65947f8d9f17ae735c7ae7bec174e349522cdf7a1260f9f227b24`.
- **contract** `juno17c5ucyukaf9heseh7gnyjgmwd3jtz6gegm039ezkjhnx6zx526hqq0738c`;
  genesis member = deployer (weight 10 000), `{"list_members":{}}` to inspect.
- **Bud** child `juno1z3295emhfln6t6kfzft5w4xwnjvhtp7va3t6xc` (weight 100)
  with the MAYO-2 test vector pk; `mayo_pk_hash` =
  `3f245334926d8355301d648fbdf6e3364cc7095594f91668afc81b8aafdb869a`.
- **`VerifyMayoAttestation`** delivered at height 373 277,
  `gas_used=309 076`, `success=true` — first on-chain post-quantum
  (MAYO-2) signature verification on the devnet.
- txhash note: the chain's `BroadcastTx` response omits the hash, so the
  relayer now falls back to `sha256(tx_bytes)` (CometBFT convention).
