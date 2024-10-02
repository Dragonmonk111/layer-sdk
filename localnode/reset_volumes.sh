#!/bin/bash

set -eux

ALPINE="alpine:latest"

SUDO="sudo"
if groups | grep -q docker; then
  SUDO=""
fi

SCRIPT_DIR="$(realpath "$(dirname "$0")")"

# reset the elastic search data
ESDATA_VOL=localnode_esdata
$SUDO docker volume rm -f "$ESDATA_VOL"


ABCI_VOL=lay3r_data
$SUDO docker volume rm -f "$ABCI_VOL"
$SUDO docker volume create "$ABCI_VOL"

# copy the data here
S=$($SUDO docker run --rm -d -v "$ABCI_VOL:/mnt" "$ALPINE" sleep 20)
$SUDO docker cp "$SCRIPT_DIR/abci/config" "$S:/mnt"
# register everything as root
$SUDO docker exec "$S" chown -R 0:0 /mnt
$SUDO docker exec "$S" ls -l /mnt/config
$SUDO docker kill "$S"


COMET_VOL=comet_data
$SUDO docker volume rm -f "$COMET_VOL"
$SUDO docker volume create "$COMET_VOL"

# copy the data here
C=$($SUDO docker run --rm -d -v "$COMET_VOL:/mnt" "$ALPINE" sleep 20)
$SUDO docker cp "$SCRIPT_DIR/comet/config" "$C:/mnt"
$SUDO docker cp "$SCRIPT_DIR/comet/data" "$C:/mnt"
# register everything as tmuser
$SUDO docker exec "$C" chown -R 100:1000 /mnt
$SUDO docker exec "$C" ls -al /mnt
$SUDO docker exec "$C" ls -l /mnt/data
$SUDO docker kill "$C"


WASM_VOL=wasmatic_data
$SUDO docker volume rm -f "$WASM_VOL"
$SUDO docker volume create "$WASM_VOL"

# copy the data here
W=$($SUDO docker run --rm -d -v "$WASM_VOL:/mnt" "$ALPINE" sleep 20)
$SUDO docker cp "$SCRIPT_DIR/wasmatic/"* "$W:/mnt"
# register everything as root
$SUDO docker exec "$W" chown -R 0:0 /mnt
$SUDO docker exec "$W" ls -l /mnt
$SUDO docker kill "$W"
