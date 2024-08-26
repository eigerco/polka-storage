################
##### Chef
FROM rust:1.77 AS chef
RUN cargo install cargo-chef
WORKDIR /app

################
##### Planer
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

################
##### Builder
FROM chef AS builder

RUN apt update && apt upgrade -y
RUN apt install -y protobuf-compiler
RUN apt install -y clang

# Copy required files
COPY --from=planner /app/recipe.json recipe.json
COPY --from=planner /app/rust-toolchain.toml rust-toolchain.toml
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json
# Build application
COPY . .
RUN cargo build --release --features polka-storage-runtime/testnet -p polka-storage-node
RUN cargo build --release --features storagext/insecure_url -p storagext-cli

FROM debian:bookworm-slim AS runtime

ARG VCS_REF
ARG BUILD_DATE

# show backtraces
ENV RUST_BACKTRACE=1

USER root

COPY --from=builder /app/target/release/polka-storage-node /usr/local/bin
COPY --from=builder /app/target/release/storagext-cli /usr/local/bin
RUN chmod -R a+rx "/usr/local/bin"

RUN useradd -m -u 1000 -U -s /bin/sh -d /eiger eiger
USER eiger
# check if executable works in this container
RUN /usr/local/bin/polka-storage-node --version

# 30333 for parachain p2p
# 30334 for relaychain p2p
# 9933 for RPC port
# 9944 for Websocket
# 9615 for Prometheus (metrics)

EXPOSE 30333 30334 9933 9944 9615
# mount-point for saving state of the parachain (not required for ZombieNet)
VOLUME ["/eiger"]
LABEL co.eiger.image.authors="releases@eiger.co" \
	co.eiger.image.vendor="Eiger" \
	co.eiger.image.title="Polka Storage Parachain" \
	co.eiger.image.description="Parachain Node binary for Polka Storage, without specs. For local ZombieNet usage only." \
	co.eiger.image.source="https://github.com/eigerco/polka-storage/blob/${VCS_REF}/docker/dockerfiles/parachain/Dockerfile" \
	co.eiger.image.revision="${VCS_REF}" \
	co.eiger.image.created="${BUILD_DATE}" \
	co.eiger.image.documentation="https://github.com/eigerco/polka-storage"

ENTRYPOINT ["/usr/local/bin/polka-storage-node"]
