#!/usr/bin/env bash
set -e

P2P_PUBLIC_KEY="./examples/storage_provider_public.pem"
P2P_PRIVATE_KEY="./examples/storage_provider_private.pem"

# Generate ED25519 private key
openssl genpkey -algorithm ED25519 -out "$P2P_PRIVATE_KEY"
# -outpubkey is only available in OpenSSL 3.4.0 onwards
# https://github.com/openssl/openssl/commit/6c03fa21ed4bbc9fd6d3013fdf9f4646d231f831
openssl pkey -in "$P2P_PRIVATE_KEY" -pubout -out "$P2P_PUBLIC_KEY"