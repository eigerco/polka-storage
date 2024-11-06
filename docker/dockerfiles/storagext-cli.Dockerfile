################
##### Chef
FROM rust:1.77 AS chef
RUN cargo install cargo-chef
WORKDIR /app

################
##### Planner
FROM chef AS planner
COPY . .
RUN cargo chef prepare --bin storagext-cli --recipe-path recipe.json

################
##### Builder
FROM chef AS builder

# Copy required files
COPY --from=planner /app/recipe.json recipe.json
COPY --from=planner /app/rust-toolchain.toml rust-toolchain.toml
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --bin storagext-cli --release --recipe-path recipe.json
# Build application
COPY . .
RUN cargo build --release --features storagext/insecure_url --bin storagext-cli

################
##### Runtime
FROM debian:bookworm-slim AS runtime
ARG VCS_REF
ARG BUILD_DATE

LABEL co.eiger.image.authors="releases@eiger.co" \
    co.eiger.image.vendor="Eiger" \
    co.eiger.image.title="Storagext CLI" \
    co.eiger.image.revision="${VCS_REF}" \
    co.eiger.image.created="${BUILD_DATE}" \
    co.eiger.image.documentation="https://github.com/eigerco/polka-storage"

WORKDIR /app
COPY --from=builder /app/target/release/storagext-cli /usr/local/bin
