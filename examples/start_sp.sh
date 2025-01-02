#!/usr/bin/env bash
set -e

trap "trap - SIGTERM && kill -- -$$" SIGINT SIGTERM EXIT

# requires the testnet to be running!
export DISABLE_XT_WAIT_WARNING=1

CLIENT="//Alice"
PROVIDER="//Charlie"

# Setup balances
RUST_LOG=debug target/release/storagext-cli --sr25519-key "$CLIENT" market add-balance 250000000000 &
RUST_LOG=debug target/release/storagext-cli --sr25519-key "$PROVIDER" market add-balance 250000000000 &
# We can process a transaction by charlie and alice, but we can't in the same transaction
# register one of them as the storage provider
wait

# It's a test setup based on the local verifying keys, everyone can run those extrinsics currently.
# Each of the keys is different, because the processes are running in parallel.
# If they were running in parallel on the same account, they'd conflict with each other on the transaction nonce.
RUST_LOG=debug target/release/storagext-cli --sr25519-key "//Charlie" storage-provider register "peer_id" &
RUST_LOG=debug target/release/storagext-cli --sr25519-key "//Alice" proofs set-porep-verifying-key @2KiB.porep.vk.scale &
RUST_LOG=debug target/release/storagext-cli --sr25519-key "//Bob" proofs set-post-verifying-key @2KiB.post.vk.scale &
wait

RUST_LOG=debug target/release/polka-storage-provider-server --sr25519-key "$PROVIDER" --seal-proof "2KiB" --post-proof "2KiB" --porep-parameters 2KiB.porep.params --post-parameters 2KiB.post.params
