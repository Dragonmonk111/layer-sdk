# Testing GRPC

Run the whole setup including gateway with:

`docker compose up`

Now, check the following endpoints:

```bash
curl localhost:1317/cosmos/bank/v1beta1/balances/slay3r1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmvk3r3j | jq .
```

```bash
curl localhost:1317/cosmos/auth/v1beta1/accounts/slay3r1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmvk3r3j | jq .
```

Run the integration tests and check again, along with the following cosmwasm ones:

**TODO**

```bash
# Is this right? Should it be snake case?
curl localhost:1317/cosmwasm/wasm/v1/code | jq .code_infos

# Warning, this dumps entire wasm blob now
curl localhost:1317/cosmwasm/wasm/v1/code/1 | jq .

# Get actual contracts
curl localhost:1317/cosmwasm/wasm/v1/code/1/contracts | jq .contracts

# Replace with address from your contract
curl localhost:1317/cosmwasm/wasm/v1/contract/slay3r13xthx4g4vjyp43gnwxk0mw4zeuvp96ljhh7q0pec9znj409er0csv5pw6v | jq .
```

```bash
echo -n '{"balance":{"address":"slay3r1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmvk3r3j"}}' | base64 -w0

curl localhost:1317/cosmwasm/wasm/v1/contract/slay3r13xthx4g4vjyp43gnwxk0mw4zeuvp96ljhh7q0pec9znj409er0csv5pw6v/smart/eyJiYWxhbmNlIjp7ImFkZHJlc3MiOiJzbGF5M3IxcGtwdHJlN2Zka2w2Z2Zyemxlc2pqdmh4aGxjM3I0Z212azNyM2oifX0= | jq -r .data | base64 -d
```

This is a basic end-to-end that they are working.

## Tendermint queries

Test that basic tendermint node queries are working:

```bash
# Sample cometbft endpoints
curl localhost:1317/cosmos/base/tendermint/v1beta1/syncing
curl localhost:1317/cosmos/base/tendermint/v1beta1/node_info | jq .
curl localhost:1317/cosmos/base/tendermint/v1beta1/blocks/latest
curl localhost:1317/cosmos/base/tendermint/v1beta1/blocks/123 | jq .

# Note, this should have a subfield `tx: []` rather than omitting when empty
curl localhost:1317/cosmos/base/tendermint/v1beta1/blocks/latest | jq .block.data

curl localhost:26657/status | jq .result.sync_info

# Example for rpc error code
curl localhost:1317/cosmos/tx/v1beta1/decode/amino
```