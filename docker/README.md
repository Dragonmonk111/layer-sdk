# Docker files

## Preparation

Pull the following images:

```bash
docker pull alpine:latest
docker pull rust:1.70-bookworm
docker pull debian:bookworm-slim
```

Build the local code:

```bash
docker build . -f docker/Dockerfile.pulsariumd -t pulsar/pulsariumd:latest
```

## Run With Docker Compose

Prepare some volumes. These will be used between runs, for some (temporary) stored state.
We use some dummy data from the integration tests. You could do something better here later.
The key point is creating some named volumes so we can restart without losing state.

```bash
./scripts/reset_volumes.sh
```

Start up with Docker Compose:

```bash
docker compose up
```

This is quite noisy with CometBFT spam, so you can check in another terminal:

```bash
docker compose logs -f pulsariumd
```

You should be able to see the jaegar traces at http://localhost:8080

Test out with [integration tests](../integration/README.md) or connect with a client.



### Stopping

```bash
docker compose down
```

### Resetting the chain

```bash
docker compose rm
./scripts/reset_volumes.sh
```