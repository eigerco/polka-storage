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


for i in $(seq 194 196);
do
    # if we try to prove commit 6 in a single row then we're done.
    # we need to throttle prove commits.
    DEAL_JSON=$(
        jq -n \
    --arg piece_cid "$PIECE_CID" \
    --argjson start_block "$i" \
    --argjson piece_size "$PIECE_SIZE" \
    '{
            "piece_cid": $piece_cid,
            "piece_size": $piece_size,
            "client": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
            "provider": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
            "label": "",
            "start_block": $start_block,
            "end_block": 250,
            "storage_price_per_block": 500,
            "provider_collateral": 1250,
            "state": "Published"
        }'
    )
    SIGNED_DEAL_JSON="$(RUST_LOG=error target/release/polka-storage-provider-client sign-deal --sr25519-key "$CLIENT" "$DEAL_JSON")"

    DEAL_CID="$(RUST_LOG=error target/release/polka-storage-provider-client propose-deal "$DEAL_JSON")"
    echo "-------------------------- Uploading deal $i..."
    echo
    curl -X PUT -F "upload=@$INPUT_FILE" "http://localhost:8001/upload/$DEAL_CID"

    echo
    echo "-------------------------- Publishing deal $i..."
    target/release/polka-storage-provider-client publish-deal "$SIGNED_DEAL_JSON" &
    sleep 6
done





for i in $(seq 197 200);
do
    # if we try to prove commit 6 in a single row then we're done.
    # we need to throttle prove commits.
    DEAL_JSON=$(
        jq -n \
    --arg piece_cid "$PIECE_CID" \
    --argjson start_block "$i" \
    --argjson piece_size "$PIECE_SIZE" \
    '{
            "piece_cid": $piece_cid,
            "piece_size": $piece_size,
            "client": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
            "provider": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
            "label": "",
            "start_block": $start_block,
            "end_block": 250,
            "storage_price_per_block": 500,
            "provider_collateral": 1250,
            "state": "Published"
        }'
    )
    SIGNED_DEAL_JSON="$(RUST_LOG=error target/release/polka-storage-provider-client sign-deal --sr25519-key "$CLIENT" "$DEAL_JSON")"

    DEAL_CID="$(RUST_LOG=error target/release/polka-storage-provider-client propose-deal "$DEAL_JSON")"
    echo "-------------------------- Uploading deal $i..."
    echo
    curl -X PUT -F "upload=@$INPUT_FILE" "http://localhost:8001/upload/$DEAL_CID"

    echo
    echo "-------------------------- Publishing deal $i..."
    target/release/polka-storage-provider-client publish-deal "$SIGNED_DEAL_JSON" &
    sleep 6
done


# wait until user Ctrl+Cs so that the commitment can actually be calculated
wait
