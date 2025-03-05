#!/usr/bin/env bash
set -e

trap "trap - SIGTERM && kill -- -$$" SIGINT SIGTERM EXIT

# requires the testnet to be running!
export DISABLE_XT_WAIT_WARNING=1

mkdir -p /tmp/polka-storage-provider

CLIENT="//Alice"
PROVIDER="//Charlie"
P2P_ADDRESS="/ip4/127.0.0.1/tcp/62649"
P2P_PUBLIC_KEY="./examples/storage_provider_public.pem"
P2P_PRIVATE_KEY="./examples/storage_provider_private.pem"
P2P_BOOTSTRAP_PUBLIC_KEY="/tmp/zombienet/charlie-public.pem"
# Config file location
CONFIG="/tmp/polka-storage-provider/config.toml"

# Generate Peer ID
P2P_SP_PEER_ID="$(target/release/polka-storage-provider-client generate-peer-id --pubkey "$P2P_PUBLIC_KEY")"

echo "Generated new peer ID for $PROVIDER: $P2P_SP_PEER_ID"

# Get bootstrap P2P Peer ID. This works after running zombienet locally or in kubernetes
P2P_BOOTSTRAP_PEER_ID="$(target/release/polka-storage-provider-client generate-peer-id --pubkey "$P2P_BOOTSTRAP_PUBLIC_KEY")"

echo "Peer ID for bootstrap node: $P2P_BOOTSTRAP_PEER_ID"

# Setup balances
RUST_LOG=debug target/release/storagext-cli --sr25519-key "$CLIENT" market add-balance 250000000000 &
RUST_LOG=debug target/release/storagext-cli --sr25519-key "$PROVIDER" market add-balance 250000000000 &
# We can process a transaction by charlie and alice, but we can't in the same transaction
# register one of them as the storage provider
wait

# It's a test setup based on the local verifying keys, everyone can run those extrinsics currently.
# Each of the keys is different, because the processes are running in parallel.
# If they were running in parallel on the same account, they'd conflict with each other on the transaction nonce.
RUST_LOG=debug target/release/storagext-cli --sr25519-key "//Charlie" storage-provider register --post-proof "8MiB" "$P2P_SP_PEER_ID" &
RUST_LOG=debug target/release/storagext-cli --sr25519-key "//Alice" proofs set-porep-verifying-key --registered-proof 8MiB @8MiB.porep.vk.scale &
RUST_LOG=debug target/release/storagext-cli --sr25519-key "//Bob" proofs set-post-verifying-key --registered-proof 8MiB @8MiB.post.vk.scale &
wait

echo "seal_proof = '8MiB'
post_proof = '8MiB'
porep_parameters = '8MiB.porep.params'
post_parameters = '8MiB.post.params'
rendezvous_point_address = '$P2P_ADDRESS'
p2p_key = '@$P2P_PRIVATE_KEY'
rendezvous_point = '$P2P_BOOTSTRAP_PEER_ID'
[sealing_configuration]
fill_threshold = 80
wait_deals_delay = '1h'
pre_commit_submission_slack = '1m'" > "$CONFIG"

RUST_LOG="polka_storage_provider_server=debug" target/release/polka-storage-provider-server \
    --sr25519-key "$PROVIDER" \
    --config "$CONFIG"
