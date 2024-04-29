# Testing GRPC

Run the whole setup including gateway with:

`docker compose up`

Now, check the following endpoints:

```bash
curl localhost:1317/cosmos/bank/v1beta1/balances/pulsar1pkptre7fdkl6gfrzlesjjvhxhlc3r4gm6k5p3l | jq .
```

```bash
curl localhost:1317/cosmos/auth/v1beta1/accounts/pulsar1pkptre7fdkl6gfrzlesjjvhxhlc3r4gm6k5p3l | jq .
```

Run the integration tests and check again, along with the following cosmwasm ones:

```bash
curl localhost:1317/cosmwasm/wasm/v1/contract/pulsar16jxkxy3zx9ac7e7ykh8sek0mnu8d69v4hz90xa2m2nc9fudhfgzsftqzfr | jq .
```

```bash
curl localhost:1317/cosmwasm/wasm/v1/code/1 | jq .
```

```bash
echo -n '{"balance":{"address":"pulsar1pkptre7fdkl6gfrzlesjjvhxhlc3r4gm6k5p3l"}}' | base64 -w0

curl localhost:1317/cosmwasm/wasm/v1/contract/pulsar16jxkxy3zx9ac7e7ykh8sek0mnu8d69v4hz90xa2m2nc9fudhfgzsftqzfr/smart/eyJiYWxhbmNlIjp7ImFkZHJlc3MiOiJwdWxzYXIxcGtwdHJlN2Zka2w2Z2Zyemxlc2pqdmh4aGxjM3I0Z202azVwM2wifX0= | jq -r .data | base64 -d
```

This is a basic end-to-end that they are working.