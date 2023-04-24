# LCD Server

This is a simple "REST" server or "Light Client Daemon" that mirrors some legacy API used in
pre-0.40 versions of Cosmos SDK. This is needed for Keplr compatibility and will be maintained
just enough to support Keplr.

## Usage

Development mode:

```
cd app/lcd
cargo run -- -h
```

Production mode:

```
cargo build --release
du -sh ./target/release/pulse-lcd
./target/release/pulse-lcd -h
```

### Flags

`-r <rpc_server>`: Address of the tendermint RPC port this proxies to

### Configuration file

You can include a [`Rocket.toml` file](../../Rocket.toml) in the directory you run 
the binary in(or the parent directory) to include configuration option.

This config format is [defined here](https://rocket.rs/v0.5-rc/guide/configuration/#configuration)
and also supports `ROCKET_` environmental variables for configuration.
