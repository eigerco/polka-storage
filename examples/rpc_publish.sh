#!/usr/bin/env bash

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
INPUT_TMP_FILE="/tmp/$INPUT_FILE.car"

target/release/mater-cli convert -q --overwrite "$INPUT_FILE" "$INPUT_TMP_FILE" &&
INPUT_COMMP="$(target/release/polka-storage-provider-client utils commp "$INPUT_TMP_FILE")"
PIECE_CID="$(echo "$INPUT_COMMP" | jq -r ".cid")"
PIECE_SIZE="$(echo "$INPUT_COMMP" | jq ".size")"


# Setup balances
target/release/storagext-cli --sr25519-key "$CLIENT" market add-balance 250000000000 &
target/release/storagext-cli --sr25519-key "$PROVIDER" market add-balance 250000000000 &
# We can process a transaction by charlie and alice, but we can't in the same transaction
# register one of them as the storage provider
wait

# Register the SP
target/release/storagext-cli --sr25519-key "//Charlie" storage-provider register "peer_id"

DEAL_JSON=$(
    jq -n \
   --arg piece_cid "$PIECE_CID" \
   --argjson piece_size "$PIECE_SIZE" \
   '{
        "piece_cid": $piece_cid,
        "piece_size": $piece_size,
        "client": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
        "provider": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
        "label": "",
        "start_block": 100000,
        "end_block": 100050,
        "storage_price_per_block": 500,
        "provider_collateral": 1250,
        "state": "Published"
    }'
)
SIGNED_DEAL_JSON="$(RUST_LOG=error target/release/polka-storage-provider-client client sign-deal --sr25519-key "$CLIENT" "$DEAL_JSON")"

(RUST_LOG=trace target/release/polka-storage-provider-server --sr25519-key "$PROVIDER" --post-proof "2KiB") &
sleep 5 # gives time for the server to start

DEAL_CID="$(RUST_LOG=error target/release/polka-storage-provider-client client propose-deal "$DEAL_JSON")"
echo "$DEAL_CID"

# Regular upload
# curl --upload-file "$INPUT_FILE" "http://localhost:8001/upload/$DEAL_CID"

# Multipart upload
curl -X PUT -F "upload=@$INPUT_FILE" "http://localhost:8001/upload/$DEAL_CID"

target/release/polka-storage-provider-client client publish-deal "$SIGNED_DEAL_JSON"

