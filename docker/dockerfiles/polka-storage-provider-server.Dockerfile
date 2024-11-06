################
##### Chef
FROM rust:1.77 AS chef
RUN cargo install cargo-chef
WORKDIR /app

################
##### Planner
FROM chef AS planner
COPY . .
RUN cargo chef prepare --bin polka-storage-provider-server --recipe-path recipe.json

################
##### Builder
FROM chef AS builder

RUN apt-get update && apt-get upgrade -y
RUN apt-get install -y libhwloc-dev \
    opencl-headers \
    ocl-icd-opencl-dev \
    protobuf-compiler \
    clang \
    build-essential \
    git

# Copy required files
COPY --from=planner /app/recipe.json recipe.json
COPY --from=planner /app/rust-toolchain.toml rust-toolchain.toml
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --bin polka-storage-provider-server --release --recipe-path recipe.json
# Build application
COPY . .
RUN cargo build --release --bin polka-storage-provider-server

################
##### Runtime
FROM debian:bookworm-slim AS runtime
ARG VCS_REF
ARG BUILD_DATE

LABEL co.eiger.image.authors="releases@eiger.co" \
    co.eiger.image.vendor="Eiger" \
    co.eiger.image.title="Polka Storage Provider Server" \
    co.eiger.image.revision="${VCS_REF}" \
    co.eiger.image.created="${BUILD_DATE}" \
    co.eiger.image.documentation="https://github.com/eigerco/polka-storage"

WORKDIR /app
COPY --from=builder /app/target/release/polka-storage-provider-server /usr/local/bin
