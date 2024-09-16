#!/bin/bash

set -eux

ALPINE="alpine/curl:latest"
COMET="cometbft/cometbft:v0.38.12"

# NOTE: override network to connect to by selecting this 
# RPC=${RPC:-https://rpc.dev-cav3.net}
# RPC=${RPC:-http://172.17.0.1:26657}
RPC=${RPC:-http://localhost:26657} # from host perspective
P2P=${P2P:-172.17.0.1:26656} # from docker perspective

SUDO="sudo"
if groups | grep -q docker; then
  SUDO=""
fi

SCRIPT_DIR="$(realpath "$(dirname "$0")")"

# TODO: get the proper node to sync to - where do we set this?

ABCI_VOL=peer_lay3r_data
$SUDO docker volume rm -f "$ABCI_VOL"
$SUDO docker volume create "$ABCI_VOL"

# copy the data here
S=$($SUDO docker run --rm -d -v "$ABCI_VOL:/mnt" "$ALPINE" sleep 100)
$SUDO docker cp "$SCRIPT_DIR/abci/config" "$S:/mnt"
# register everything as root
$SUDO docker exec "$S" chown -R 0:0 /mnt
$SUDO docker kill "$S"

COMET_VOL=peer_comet_data
$SUDO docker volume rm -f "$COMET_VOL"
$SUDO docker volume create "$COMET_VOL"

# initialize data
$SUDO docker run --rm -d -v "$COMET_VOL:/cometbft" "$COMET" init

# download the genesis here (requires curl and jq locally...)
curl -s "$RPC/genesis" | jq .result.genesis > "$SCRIPT_DIR/genesis.json" 
PEER=$(curl -s "$RPC/status" | jq -r .result.node_info.id)
cat genesis.json

# copy the config here
C=$($SUDO docker run --rm -d -v "$COMET_VOL:/mnt" "$ALPINE" sleep 100)
$SUDO docker cp "$SCRIPT_DIR/comet/config/config.toml" "$C:/mnt/config"
$SUDO docker cp "$SCRIPT_DIR/genesis.json" "$C:/mnt/config"
$SUDO rm "$SCRIPT_DIR/genesis.json"
# udpate the persistent peers
$SUDO docker exec "$C" sed -i -e "s/PEERS_HERE/$PEER@$P2P/" /mnt/config/config.toml
# register everything as tmuser
$SUDO docker exec "$C" chown -R 100:1000 /mnt

$SUDO docker kill "$C"



