#!/usr/bin/env bash

# Sets PIECE_CID and PIECE_SIZE based on the input file.
function set_piece_vars {
    INPUT_FILE=$1
    INPUT_TMP_FILE=$2

    # Convert file to CARv2 format  
    echo "Converting ${INPUT_FILE} to CARv2 format..."
    target/release/mater-cli convert -q --overwrite "$INPUT_FILE" "$INPUT_TMP_FILE" > /dev/null 2>&1

    echo "Calculating COMMP..."
    COMMP="$(target/release/polka-storage-provider-client proofs commp ${INPUT_TMP_FILE})"
    PIECE_CID=$(jq -r '.cid' <<< "$COMMP")
    PIECE_SIZE=$(jq -r '.size' <<< "$COMMP")
}

# Sets latest finalized block from the chain
function set_latest_block {
    LATEST_BLOCK="$(./target/release/storagext-cli system get-height --wait-for-finalization | awk '{print $3}')"
}

# Full publish deal flow.
# INPUT_FILE should be set before calling.
function publish_deal {
    CLIENT=$1
    PROVIDER=$2
    PIECE_CID=$3
    PIECE_SIZE=$4
    START_BLOCK=$5
    END_BLOCK=$6
    CLIENT_ACCOUNT=$7
    PROVIDER_ACCOUNT=$8
    INPUT_FILE=$9
    PORT="${10:-8001}"

    URL="http://127.0.0.1:"$PORT""

    DEAL_JSON=$(
        jq -n \
    --arg piece_cid "$PIECE_CID" \
    --argjson piece_size "$PIECE_SIZE" \
    --argjson start_block "$START_BLOCK" \
    --argjson end_block "$END_BLOCK" \
    --arg client "$CLIENT_ACCOUNT" \
    --arg provider "$PROVIDER_ACCOUNT" \
    '{
            "piece_cid": $piece_cid,
            "piece_size": $piece_size,
            "client": $client,
            "provider": $provider,
            "label": "",
            "start_block": $start_block,
            "end_block": $end_block,
            "storage_price_per_block": 500,
            "state": "Published"
        }'
    )

    echo ""$CLIENT" is signing deal..."
    SIGNED_DEAL_JSON="$(RUST_LOG=error target/release/polka-storage-provider-client sign-deal --sr25519-key "$CLIENT" "$DEAL_JSON")"

    echo "Proposing deal between "$CLIENT_ACCOUNT" and "$PROVIDER_ACCOUNT"..."
    DEAL_CID="$(curl -s -X POST -H "Content-Type: application/json" -d "$DEAL_JSON" "${URL}/api/v0/propose_deal" | jq -r)"

    echo "Uploading deal with CID "$DEAL_CID" to storage provider..."
    curl -s -X PUT -F "upload=@$INPUT_FILE" "${URL}/api/v0/upload/$DEAL_CID" > /dev/null 2>&1

    echo "Publishing deal to ${PROVIDER}..."
    curl -s -X POST -H "Content-Type: application/json" -d "$SIGNED_DEAL_JSON" "${URL}/api/v0/publish_deal" > /dev/null 2>&1
}