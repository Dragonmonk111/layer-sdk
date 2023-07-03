#!/bin/bash

set -eux

ALPINE="alpine:latest"

ABCI_VOL=pulsar_data
docker volume rm -f "$ABCI_VOL"
docker volume create "$ABCI_VOL"

# copy the data here
C=$(docker run --rm -d -v "$ABCI_VOL:/mnt" "$ALPINE" sleep 100)
docker cp ./docker/config "$C:/mnt"
# register everything as root
docker exec "$C" chown -R 0:0 /mnt
docker exec "$C" ls -l /mnt/config
docker kill "$C"

COMET_VOL=comet_data
docker volume rm -f "$COMET_VOL"
docker volume create "$COMET_VOL"

# copy the data here
C=$(docker run --rm -d -v "$COMET_VOL:/mnt" "$ALPINE" sleep 100)
docker cp ./integration/etc/config "$C:/mnt"
docker cp ./integration/etc/data "$C:/mnt"
# register everything as tmuser
docker exec "$C" chown -R 100:1000 /mnt
docker exec "$C" ls -al /mnt
docker exec "$C" ls -l /mnt/data
docker kill "$C"