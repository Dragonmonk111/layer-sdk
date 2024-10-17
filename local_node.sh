#!/bin/bash
set -xeu


HOME_DIR=.onomy
BIN=onomyd
DENOM=anom

update_genesis () {    
    cat $HOME_DIR/config/genesis.json | jq "$1" > $HOME_DIR/config/tmp_genesis.json && mv $HOME_DIR/config/tmp_genesis.json $HOME_DIR/config/genesis.json
}

rm -rf $HOME_DIR

MNE1="amused rural desk trick safe whip first menu worth swap enhance punch spin figure elevator abandon camera idea peace nurse coyote adjust modify produce"
MNE5="pink draw film undo drama horror eternal hill team spin dolphin crane essay boost couple cereal jungle crime visa record bean knock giggle recycle"

mkdir $HOME_DIR
$BIN init god --chain-id local_3300-1 --home $HOME_DIR  2> output1

node1=$(cat output1 | jq -r '.node_id')

rm output*

echo $MNE1 | $BIN keys add val --recover --keyring-backend test --home $HOME_DIR
echo $MNE5 | $BIN keys add god --recover --keyring-backend test --home $HOME_DIR

$BIN genesis add-genesis-account $($BIN keys show god -a --keyring-backend test --home $HOME_DIR) 1000000000000000$DENOM --keyring-backend test --home $HOME_DIR 
$BIN genesis add-genesis-account $($BIN keys show val -a --keyring-backend test --home $HOME_DIR) 1000000000000$DENOM --keyring-backend test --home $HOME_DIR
$BIN gentx val 500000000$DENOM --keyring-backend test --home $HOME_DIR --chain-id local_3300-1
$BIN collect-gentxs --home $HOME_DIR
 
update_genesis '.app_state["staking"]["params"]["bond_denom"]="$DENOM"'
update_genesis '.app_state["staking"]["params"]["unbonding_time"]="240s"'
update_genesis '.app_state["crisis"]["constant_fee"]["denom"]="$DENOM"'
update_genesis '.app_state["gov"]["voting_params"]["voting_period"]="60s"'
update_genesis '.app_state["gov"]["deposit_params"]["min_deposit"][0]["denom"]="$DENOM"'
update_genesis '.app_state["mint"]["params"]["mint_denom"]="$DENOM"'

PEERS="$node1@0.0.0.0:26656"
echo $PEERS
sed -i.bak -e "s/^persistent_peers *=.*/persistent_peers = \"$PEERS\"/" $HOME_DIR/config.toml
sed -i.bak -e "s/127\.0\.0\.1/0.0.0.0/" $HOME_DIR/config.toml
sed -i.bak -e "s/127\.0\.0\.1/0.0.0.0/" $HOME_DIR/app.toml
sed -i.bak -e "s/addr_book_strict = true/addr_book_strict = false/" $HOME_DIR/config.toml
sed -i.bak -e "s/cors_allowed_origins = \[\]/cors_allowed_origins = [\"*\"]/" $HOME_DIR/config.toml
sed -i.bak -e '/^\[api\]$/,/^\[/ s/enable = false/enable = true/' $HOME_DIR/app.toml

sudo tee /etc/systemd/system/$BIN.service > /dev/null <<EOF
[Unit]
Description=$BIN
After=network.target

[Service]
ExecStart=$(which $BIN) run

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable $BIN && systemctl restart $BIN
sudo journalctl -fu $BIN