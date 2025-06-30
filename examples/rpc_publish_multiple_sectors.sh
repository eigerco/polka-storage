#!/usr/bin/env bash
set -e
set -x

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


for i in $(seq 100 100 | tac);
do
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
            "state": "Published"
        }'
    )
    SIGNED_DEAL_JSON="$(RUST_LOG=error target/release/polka-storage-provider-client sign-deal --sr25519-key "$CLIENT" "$DEAL_JSON")"

    DEAL_CID="$(curl -X POST -H "Content-Type: application/json" -d "$DEAL_JSON" 'http://127.0.0.1:8001/api/v0/propose_deal' | jq -r)"
    echo "-------------------------- Uploading deal $i..."
    echo
    curl -X PUT -F "upload=@$INPUT_FILE" "http://localhost:8001/upload/$DEAL_CID"

    echo
    echo "-------------------------- Publishing deal $i..."
    target/release/polka-storage-provider-client publish-deal "$SIGNED_DEAL_JSON" &
done

# wait until user Ctrl+Cs so that the commitment can actually be calculated
wait
