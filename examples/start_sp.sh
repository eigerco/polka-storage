#!/usr/bin/env bash
set -e

if [ "$#" -ne 1 ]; then
    echo "$0: input file required"
    exit 1
fi

if [ -z "$1" ]; then
    echo "$0: input file cannot be empty"
    exit 1
fi

trap "trap - SIGTERM && kill -- -$$" SIGINT SIGTERM EXIT

# requires the testnet to be running!
export DISABLE_XT_WAIT_WARNING=1

CLIENT="//Alice"
PROVIDER="//Charlie"

INPUT_FILE="$1"
INPUT_FILE_NAME="$(basename "$INPUT_FILE")"
INPUT_TMP_FILE="/tmp/$INPUT_FILE_NAME.car"

target/release/mater-cli convert -q --overwrite "$INPUT_FILE" "$INPUT_TMP_FILE" &&
INPUT_COMMP="$(target/release/polka-storage-provider-client proofs commp "$INPUT_TMP_FILE")"
PIECE_CID="$(echo "$INPUT_COMMP" | jq -r ".cid")"
PIECE_SIZE="$(echo "$INPUT_COMMP" | jq ".size")"


# Setup balances
RUST_LOG=debug target/release/storagext-cli --sr25519-key "$CLIENT" market add-balance 250000000000 &
RUST_LOG=debug target/release/storagext-cli --sr25519-key "$PROVIDER" market add-balance 250000000000 &
# We can process a transaction by charlie and alice, but we can't in the same transaction
# register one of them as the storage provider
wait

RUST_LOG=debug target/release/storagext-cli --sr25519-key "//Charlie" storage-provider register "peer_id" &
RUST_LOG=debug target/release/storagext-cli --sr25519-key "//Alice" proofs set-porep-verifying-key @2KiB.porep.vk.scale &
RUST_LOG=debug target/release/storagext-cli --sr25519-key "//Bob" proofs set-post-verifying-key @2KiB.post.vk.scale &
wait

RUST_LOG=debug target/release/polka-storage-provider-server --sr25519-key "$PROVIDER" --seal-proof "2KiB" --post-proof "2KiB" --porep-parameters 2KiB.porep.params --post-parameters 2KiB.post.params
