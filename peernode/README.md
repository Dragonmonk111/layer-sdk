# Peernode

The peernode directory contains configuration and scripts to run a non-signing "peer node",
which can connect to localnode (default), or devnet.

It assumes you have `docker` and `docker compose` installed locally. If you can run
`docker` as your current user (not just `root`), you can remove `sudo` from the following commands.

It also assumes you have `curl` and `jq` installed locally

## Running a Node

Before starting peernode the first time, and anytime you wish to restart the blockchain, run
the following command. Without it, we maintain the blockchain state between restarts.

```bash
./peernode/reset_volumes.sh
```

We also need to build the docker images for the chain (slay3rd and gateway) one
time before running a peernode, and each time you want to update the codebase.

```bash
./scripts/build_docker.sh
```

### Executing a node

To run without a facuet and just in-memory tracing, run the following:

```bash
# Starting
./peernode/run.sh
sudo docker ps

# Stopping
./peernode/stop.sh
sudo docker ps -a
```

### Connecting to dev net

Dev-net has a different config, so you need to reset volumes to prepare it for that. Try the following:

```bash
RPC=https://rpc.dev-cav3.net P2P=p2p.dev-cav3.net:26656 ./peernode/reset_volumes.sh
```

(TODO: enable p2p url and firewall on dev-net)

## Interacting with a Node

There are a few ways you can interact with a peernode.

### Javascript Tests

The [`js`](../js/) directory contains some integration tests for peernode, writing in CosmJS.
Make sure to enable the faucet to run them all, which means `run_all.sh` above. Then:

```bash
npm ci
npm run test
```

See [the module's README](../js/README.md) for more information
