# Testing

In order to test the LCD, we will need to run a few services together and then connect the Keplr wallet.

## Prerequisites

### Tendermint

You should have Go 1.18+ installed and configured (eg `$HOME/go/bin` added to `$PATH`).
[Install Tendermint](https://github.com/tendermint/tendermint/blob/main/docs/introduction/install.md)
according to the docs, but make sure to `git checkout v0.34.24` before `make install`.

Once you have achieved that, check the binary can be called and is in the proper version:

```shell
$ tendermint version
v0.34.24
```

### Basecoin-rs

Until Pulsarium is developed, we will use [basecoin-rs](https://github.com/informalsystems/basecoin-rs)
as a placeholder ABCI app to demonstrate.

```shell
git clone https://github.com/informalsystems/basecoin-rs.git
cd basecoin-rs
cargo install --path .
```

Once it is installed, check you can execute:

```shell
basecoin --help
```

### Pulse LCD

We will run the code in this directory. Make sure you've cloned it locally.

### Keplr

Open https://keplr.app in Chrome and install the chrome extension.
Make a new account (Called "Pulsarium Test" in the extension for use below).
Write down this address...

## Installation

In order to start, we will need to set up a genesis file for tendermint. Start with the basic
Tendermint setup.

```shell
tendermint init --home ~/.pulsarium
```

Now, we need to add some keys to the genesis file. Type `vi ~/.pulsarium/config/genesis.json`
and add the following element to the JSON (after `"app_hash": ""` and make sure to add a comma to that
line).

```json
  "app_state": {
    "YOUR_COSMOS_ADDRESS_FROM_KEPLR": {
      "upulse": "0x75bca00"
    }
  }
```

### Resetting Tendermint

Basecoin stores no state, so it crashes due to inconsistency once you restart. However,
you don't have to reinstall, but rather simple reset the state:

```shell
tendermint unsafe-reset-all --home ~/.pulsarium
```

## Start Services

You will need 3 different terminal shells for this.

**Shell 1**

```shell
basecoin -p 26658
```

**Shell 2**

```shell
tendermint start --home ~/.pulsarium
```

After this, you should see some blocks start to produce in those terminals.

**Shell 3**

In this directory, start the dev LCD proxy

```shell
cargo run
```

## Connect Keplr

Find the chain ID of your local devnet via: 

```shell
grep chain_id ~/.pulsarium/config/genesis.json
```

Then copy [`devnet.json`](./devnet.json) and edit the chain_id field, using the value you got above.

Now, you can install this into Keplr, by going to [Alexar Devtools](https://docs.axelar.dev/resources/keplr#add-your-custom-network)
And replacing the JSON configuration with the one in your customized `devnet.json`, and clicking "Validate Input
& Add to Keplr". 

A Keplr Pop-Up will appear and you should approve.

Now, go to Keplr, click on "Cosmos Hub" to select the chain selector, and scroll all the way to the bottom
where you see "Pulsarium DevNet" down under Beta. Select this one and check your balance.

Now, click on the "Send" button and try to send 5 PULSE to `cosmos1pt9m7mgecj0ulsv0njhafrlzn5kz059kt4r7zs`.

