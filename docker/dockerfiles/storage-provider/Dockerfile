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
RUN cargo build --release --bin polka-storage-provider

################
##### Runtime
FROM debian:bookworm-slim AS runtime
ARG VCS_REF
ARG BUILD_DATE

LABEL co.eiger.image.authors="releases@eiger.co" \
    co.eiger.image.vendor="Eiger" \
    co.eiger.image.title="Polka Storage Provider" \
    co.eiger.image.description="Provides a server used for uploading arbitrary files and downloading them as CAR files." \
    co.eiger.image.revision="${VCS_REF}" \
    co.eiger.image.created="${BUILD_DATE}" \
    co.eiger.image.documentation="https://github.com/eigerco/polka-storage"

WORKDIR /app
COPY --from=builder /app/target/release/polka-storage-provider /usr/local/bin
ENTRYPOINT ["/usr/local/bin/polka-storage-provider"]
