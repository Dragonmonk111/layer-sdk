# Localnode

The localnode directory contains configuration and scripts to run your own localnode.
It assumes you have `docker` and `docker compose` installed locally. If you can run
`docker` as your current user (not just `root`), you can remove `sudo` from the following commands.

## Running a Node

Before starting localnode the first time, and anytime you wish to restart the blockchain, run
the following command. Without it, we maintain the blockchain state between restarts.

```bash
./localnode/reset_volumes.sh
```

We also need to build the docker images for the chain (slay3rd and gateway) one
time before running a localnode, and each time you want to update the codebase.

```bash
./scripts/build_docker.sh
```

### Debugging

Sometimes (only on OSX?), it won't automatically pull the missing packages listed
in the docker compose files and you must manually pull them before running the node, or potentially building the docker images. If you get an error like this:

```
ERROR: failed to solve: debian:bookworm-slim: failed to resolve source metadata for docker.io/library/debian:bookworm-slim: error getting credentials - err: exit status 1, out: ``
```

Do the following before building:

```bash

docker pull debian:bookworm-slim
docker pull rust:1.80-bookworm
docker pull golang:1.22-bookworm
docker pull alpine:latest
```

And the following before running a node (`./localnode/run*.sh`)

```bash
# This may work?
docker compose -f ./localnode/docker-compose.yml -f ./localnode/jaeger-elastic-compose.yml pull

# If not, do this manually
docker pull cometbft/cometbft:v0.38.12
docker pull jaegertracing/all-in-one:1.59
docker pull docker.elastic.co/elasticsearch/elasticsearch:8.15.1
docker pull jaegertracing/jaeger-collector:1.59
docker pull jaegertracing/jaeger-agent:1.59
docker pull jaegertracing/jaeger-query:1.59
```

### Minimal Run

To run without a facuet and just in-memory tracing, run the following:

```bash
# Starting
./localnode/run.sh
sudo docker ps

# Stopping
./localnode/stop.sh
sudo docker ps -a
```

### Full run

If you want to include a faucet, and also store all tracing logs in a persistent elastic search engine
(exposed at port 9200), run the following:

```bash
# Starting
./localnode/run_all.sh
sudo docker ps

# Show there is data in elastic search
curl localhost:9200/_search

# Stopping
./localnode/stop.sh
sudo docker ps -a
```

Note: if you want elastic search but have errors running the faucet locally, try `run_elastic.sh` which serves elastic search at port 9200,
but doesn't start the faucet at all.

## Interacting with a Node

There are a few ways you can interact with a localnode.

### Javascript Tests

The [`js`](../js/) directory contains some integration tests for localnode, writing in CosmJS.
Make sure to enable the faucet to run them all, which means `run_all.sh` above. Then:

```bash
npm ci
npm run test
```

See [the module's README](../js/README.md) for more information

### Rust CLI

There are some custom CLI tools meant for deploying certain contracts to the testnet.
These can be found in `layer-contracts` repo in the `deploy` package.

([Full Link](https://github.com/Lay3rLabs/lay3r-contracts/tree/main/deploy))

### Golang CLI

You can also use a fork of `wasmd`, which we maintain on the
[slay3r branch of our fork](https://github.com/Lay3rLabs/wasmd/tree/slay3r).

```bash
git clone https://github.com/Lay3rLabs/wasmd.git
cd wasmd
git checkout slay3r
make install

slay3r version
```

TODO: You need to configure it to point to our server, then it should work like you are used to,
at least the bank and wasm subcommands.

Note: You must have a proper golang installation (1.20+ I think)

### Getting tokens

The "proper" way to get tokens is to hit the faucet. When you `run_all.sh`, the faucet will bind to port 8000,
and can be used [as described here](https://github.com/cosmos/cosmjs/blob/main/packages/faucet/README.md#using-the-faucet).

The easy but dirty way is to look at the [test mnemonic we use for the faucet](./faucet.env) and enter that into your wallet.
