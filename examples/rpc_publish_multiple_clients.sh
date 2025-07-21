#!/usr/bin/env bash
set -e

# This script will publish a file from every client to every storage provider.
# It is supposed to be used in tandem with start_sps.sh (which launches Alice, Bob, and Charlie)

if [ "$#" -ne 1 ]; then
    echo "$0: input file required"
    exit 1
fi

if [ -z "$1" ]; then
    echo "$0: input file cannot be empty"
    exit 1
fi

trap "trap - SIGTERM && kill -- -$$" SIGINT SIGTERM

source "$(dirname "$0")/deal_common.sh"

export DISABLE_XT_WAIT_WARNING=1

INPUT_FILE="$1"
INPUT_FILE_NAME="$(basename "$INPUT_FILE")"
INPUT_TMP_FILE="/tmp/$INPUT_FILE_NAME.car"

CLIENTS=("//David" "//Eve" "//Fred")
PROVIDERS=("//Alice" "//Bob" "//Charlie")
PORTS=("8001" "8002" "8003")

# Convert file to CARv2 and calculate COMMP only once
set_piece_vars "$INPUT_FILE" "$INPUT_TMP_FILE"

for CLIENT in "${CLIENTS[@]}"; do
    CLIENT_ACCOUNT=$(subkey inspect "${CLIENT}" --output-type json | jq -r '.ss58Address')

    for i in "${!PROVIDERS[@]}"; do
        PROVIDER="${PROVIDERS[$i]}"
        PORT="${PORTS[$i]}"
        PROVIDER_ACCOUNT=$(subkey inspect "${PROVIDER}" --output-type json | jq -r '.ss58Address')

        # Get latest finalized block for each deal
        set_latest_block
        START_BLOCK=$((LATEST_BLOCK + 20))
        END_BLOCK=$((START_BLOCK + 10000))

        # Publish the deal
        publish_deal \
            "$CLIENT" \
            "$PROVIDER" \
            "$PIECE_CID" \
            $PIECE_SIZE \
            $START_BLOCK \
            $END_BLOCK \
            "$CLIENT_ACCOUNT" \
            "$PROVIDER_ACCOUNT" \
            "$INPUT_FILE" \
            "$PORT"

        echo "Published deal: ${CLIENT} -> ${PROVIDER}"
    done
done

echo "All deals published successfully!"
