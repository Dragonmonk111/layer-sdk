# docker build . -t pulsar/pulsariumd:latest
# docker pull cometbft/cometbft:0.38.0-rc2
FROM rust:1.70-bullseye as builder
WORKDIR /usr/src/myapp
COPY . .
RUN cargo install --path ./app/pulsariumd

# TODO: use bookworm if working
FROM debian:bullseye-slim
RUN apt-get update && apt-get upgrade
RUN apt install -y libcurl4
COPY --from=builder /usr/local/cargo/bin/pulsariumd /usr/local/bin/pulsariumd
EXPOSE 26658
CMD ["pulsariumd"]

# TODO: copy some config file / dir there?
# docker run --rm pulsar/pulsariumd:latest pulsariumd --help
# docker run --rm -it pulsar/pulsariumd:latest /bin/bash
# docker run --rm pulsar/pulsariumd:latest pulsariumd --host 0.0.0.0
