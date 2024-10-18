#!/bin/bash

cometbft init --home ~/.slay3r
nano slay3r.toml

nano ~/.slay3r/config/genesis.json

cometbft start --home ~/.slay3r
slay3rd --log debug
