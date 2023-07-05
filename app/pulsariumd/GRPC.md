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

TODO

This is a basic end-to-end that they are working.