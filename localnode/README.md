# Localnode

The localnode directory contains configuration and scripts to run your own localnode.
It assumes you have `docker` and `docker compose` installed locally. If you can run
`docker` as your current user (not just `root`), you can remove `sudo` from the following commands.

## Setup

Before starting localnode the first time, and anytime you wish to restart the blockchain, run
the following command. Without it, we maintain the blockchain state between restarts.

```bash
./localnode/reset_volumes.sh
```

## Minimal Run

To run without a facuet and just in-memory tracing, run the following:

```bash
# Starting
./localnode/run.sh
sudo docker ps

# Stopping
./localnode/stop.sh
sudo docker ps -a
```

## Full run

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
