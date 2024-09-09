#!/bin/bash

set -eux

ALPINE="alpine:latest"

SUDO="sudo"
if groups | grep -q docker; then
  SUDO=""
fi

# reset the elastic search data
ESDATA_VOL=localnode_esdata
$SUDO docker volume rm -f "$ESDATA_VOL"

ABCI_VOL=lay3r_data
$SUDO docker volume rm -f "$ABCI_VOL"
$SUDO docker volume create "$ABCI_VOL"

# copy the data here
C=$($SUDO docker run --rm -d -v "$ABCI_VOL:/mnt" "$ALPINE" sleep 100)
$SUDO docker cp ./docker/config "$C:/mnt"
# register everything as root
$SUDO docker exec "$C" chown -R 0:0 /mnt
$SUDO docker exec "$C" ls -l /mnt/config
$SUDO docker kill "$C"

COMET_VOL=comet_data
$SUDO docker volume rm -f "$COMET_VOL"
$SUDO docker volume create "$COMET_VOL"

# copy the data here
C=$($SUDO docker run --rm -d -v "$COMET_VOL:/mnt" "$ALPINE" sleep 100)
$SUDO docker cp ./integration/etc/config "$C:/mnt"
$SUDO docker cp ./integration/etc/data "$C:/mnt"
# register everything as tmuser
$SUDO docker exec "$C" chown -R 100:1000 /mnt
$SUDO docker exec "$C" ls -al /mnt
$SUDO docker exec "$C" ls -l /mnt/data
$SUDO docker kill "$C"