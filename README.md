# Slay3r

Unleash the Power of Slay3r: A Developer-Friendly, Pure-Rust, CosmWasm Blockchain. Blazing-fast Speed, Interchain Connectivity via IBC, and Stellar Stability

<p align="center">
  <img width="384" height="384" src="./img/slay3r.jpg">
</p>

## Quick Start

There are scattered notes on how to run stuff spread throughout READMEs that should be organized.

Easiest is to check out [`docker-compose.yml`](./docker-compose.yml) and follow the instructions on the top to set up a local server. You will need to set up keplr (or vectis) with a mnemonic, which you can find
in [`TESTING.md`](./gateway/TESTING.md#connect-keplr).

## Application Layout

Docker compose gives a nice overview. 

We run CometBFT 0.38, which exposes tendermint-rpc on port `26657`` and p2p networking (to sync nodes) on `26656``.

In [`app/slay3rd`](./app/slay3rd/) we find the entry point for the ABCI application. When run it will bind abci to port `26658`, which cometBFT can connect to in order to run a blockchain. It also exposes Cosmos-SDK compatible gRPC endpoint on port `9090`.

In [`gateway`](./gateway/) we find the gRPC gateway, which exposes a REST API on port `1317` and forwards requests to the gRPC endpoint. This is auto-generated from the protobuf definitions in [`proto`](./proto/).
In order to regenerate the gateway, run `./scripts/build_proto_gateway` to generate new Go code from the protobuf files. Then run `go build` in the gateway directory to build the binary.

`Slay3rd` exports a lot of tracing information via open tracing. By default, we connect this to a jaeger instance. This is configured in [`app/slay3rd/src/main.rs`](app/slay3rd/src/main.rs#L72-L91) and one could update the configuration there to use any other Open Tracing compatible consumer. `docker compose up` also starts such jaeger instance and exposes a web UI on port `8080`, which gives a nice overview of activity on the chain and let's you delve into transaction and query details.


## Testing

In [`integration`](./integration/) we find the integration tests, which are written in TypeScript using CosmJS and verify that the slay3r blockchain is compatible with Cosmos clients (to some degree). These tests are run by CI and should be kept up to date with the latest changes.

These integration tests also upload some standard contracts used by Vectis TODO app, and currently the docker-compose serves this on port `8000`. 

### Integrations

A quick way to test code is not to run the full docker setup, but rather just run cometBFT in docker and locally compile and run slay3rd to update quicker. Follow the instructions in [`app/slay3rd/README.md`](./app/slay3rd/README.md#setup-configuration)

## MSRV

This requires Rust 1.73 or higher. We use the latest stable version of Rust.
