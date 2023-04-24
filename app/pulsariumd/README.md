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





