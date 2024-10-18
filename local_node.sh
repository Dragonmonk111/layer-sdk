#!/bin/bash

mkdir -p ~/.slay3r
cometbft init --home ~/.slay3r

cp ./genesis.json ~/.slay3r/config/.

cp ./slay3r.toml ~/.slay3r/config/.

cp ./priv_validation.json ~/.slay3r/config/.

slay3rd --log debug

# run this at a different terminal
# cometbft start --home ~/.slay3r
