#!/bin/bash
# Install required packages
apt install -y libhwloc-dev opencl-headers ocl-icd-opencl-dev protobuf-compiler clang git curl

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y

# Reload PATH to include Cargo's bin directory
. $HOME/.cargo/env

# Install Just
cargo install just