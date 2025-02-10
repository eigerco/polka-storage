#!/usr/bin/env bash
set -e

trap "trap - SIGTERM && kill -- -$$" SIGINT SIGTERM EXIT

# requires the testnet to be running!
export DISABLE_XT_WAIT_WARNING=1

CLIENT="//Alice"
PROVIDER="//Charlie"
P2P_PUBLIC_KEY="/tmp/public.pem"
P2P_PRIVATE_KEY="/tmp/private.pem"
P2P_ADDRESS="/ip4/127.0.0.1/tcp/62649"

# Generate ED25519 private key
openssl genpkey -algorithm ED25519 -out "$P2P_PRIVATE_KEY"
# -outpubkey is only available in OpenSSL 3.4.0 onwards
# https://github.com/openssl/openssl/commit/6c03fa21ed4bbc9fd6d3013fdf9f4646d231f831
openssl pkey -in "$P2P_PRIVATE_KEY" -pubout -out "$P2P_PUBLIC_KEY"

# Generate Peer ID
PEER_ID="$(target/release/polka-storage-provider-client generate-peer-id --pubkey "$P2P_PUBLIC_KEY")"

# Setup balances
RUST_LOG=debug target/release/storagext-cli --sr25519-key "$CLIENT" market add-balance 250000000000 &
RUST_LOG=debug target/release/storagext-cli --sr25519-key "$PROVIDER" market add-balance 250000000000 &
# We can process a transaction by charlie and alice, but we can't in the same transaction
# register one of them as the storage provider
wait

# It's a test setup based on the local verifying keys, everyone can run those extrinsics currently.
# Each of the keys is different, because the processes are running in parallel.
# If they were running in parallel on the same account, they'd conflict with each other on the transaction nonce.
RUST_LOG=debug target/release/storagext-cli --sr25519-key "//Charlie" storage-provider register "$PEER_ID" &
RUST_LOG=debug target/release/storagext-cli --sr25519-key "//Alice" proofs set-porep-verifying-key @2KiB.porep.vk.scale &
RUST_LOG=debug target/release/storagext-cli --sr25519-key "//Bob" proofs set-post-verifying-key @2KiB.post.vk.scale &
wait

echo '{
    "seal_proof": "2KiB",
    "post_proof": "2KiB",
    "porep_parameters": "2KiB.porep.params",
    "post_parameters": "2KiB.post.params",
    "p2p_key": "@/tmp/private.pem",
    "rendezvous_point_address": "/ip4/127.0.0.1/tcp/62649",
    "sealing_configuration": {
        "fill_percentage": 75
    }
}' > /tmp/storage_provider.config.json
RUST_LOG=debug target/release/polka-storage-provider-server \
    --sr25519-key "$PROVIDER" \
    --config /tmp/storage_provider.config.json
