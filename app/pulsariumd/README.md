# Pulsarium Daemon

This is the core application, and ABCI-enabled app that talks with tendermint to convert
the powerful, pure-Rust state machine into a full-fledged blockchain.

## Configuration

All values can be specified either in a `pulsarium.toml` file (located in `~/.pulsarium/config`),
via environmental variables (prefixed with `PULSE_`), or as command-line flags.
The actions listed later take precedence over those listed earlier.

Any field not defined will use defaults from the code.

You can see this via:

```shell
PULSE_LOG="debug" cargo run -- --host 0.0.0.0
```

To see the full options:

```shell
cargo run -- -h
```

## Running with CometBFT

### Install Software

You can install version `v0.38.0-alpha.2` of CometBFT by following the [installation instructions](https://github.com/cometbft/cometbft/blob/v0.38.0-alpha.2/docs/guides/install.md)

```shell
mkdir -p ~/golang
cd ~/golang
git clone https://github.com/cometbft/cometbft.git
cd cometbft
git checkout v0.38.0-alpha.2
make install

cometbft version
ls -l $(which cometbft)
# Around 26MB at v0.38.0-alpha.2!
```

You also need to install the pulsariumd binary from this repo:

```shell
cargo install --path ./app/pulsariumd

pulsariumd -h
ls -l $(which pulsariumd)
# Around 2.7MB at v0.2.0!
```

(Dev note: checking size and optimization...)

```shell
cd ./app/pulsariumd
RUSTFLAGS='-C link-arg=-s' cargo build --release
ls -l ../../target/release/pulsariumd
# Around 1.8MB at v0.2.0!
```

### Setup Configuration

Set up basic tendermint

```
rm -rf ~/.pulsarium
mkdir -p ~/.pulsarium
cometbft init --home ~/.pulsarium
```


Add the following to `~/.pulsarium/config/genesis.json`:

```json
"app_state": {
    "bank": [
        {
            "address": "pulsar1eaulhtty6er8e3huz8c4wktz82vf8krnptv9dx",
            "balance": [{
                "amount": "123456000000",
                "denom": "upulse"
            }]
        }
    ]
}
```

### First execution

One terminal:

```
cometbft start --home ~/.pulsarium
```

Second terminal:

```
pulsariumd --log debug
```

### Reset

Since we have a memory store for the app, once we stop it is out of sync with tendermint.
We need to reset and restart. (We will be able to continue later with similar version
once we have a disk store.)

```shell
cometbft unsafe-reset-all --home ~/.pulsarium
```

### Dev Mode

For debugging, let's run a quick build of `pulsariumd`:

```shell
cd ./app/pulsariumd
PULSE_LOG=debug cargo run
```

For cometbft, we also need to reset state every crash.

```
cometbft unsafe-reset-all --home ~/.pulsarium
cometbft start --home ~/.pulsarium
```


Then just keep restarting the `cometbft` and `pulsariumd` process on crash