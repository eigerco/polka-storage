#!/usr/bin/env bash
set -e

# This script will publish a file to Alice, Bob and Charlie 
# with Eve being the client.
# it is supposed to be used in tandem with start_sps.sh

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

# requires the testnet to be running!
export DISABLE_XT_WAIT_WARNING=1

INPUT_FILE="$1"
INPUT_FILE_NAME="$(basename "$INPUT_FILE")"
INPUT_TMP_FILE="/tmp/$INPUT_FILE_NAME.car"
CLIENT="//Eve"
CLIENT_ACCOUNT=$(subkey inspect "${CLIENT}" --output-type json | jq -r '.ss58Address')
declare -a ACCOUNTS=("//Alice" "//Bob" "//Charlie")
declare -a PORTS=("8001" "8002" "8003")

for i in "${!ACCOUNTS[@]}"; do
    PROVIDER="${ACCOUNTS[$i]}"
    PORT="${PORTS[$i]}"
    PROVIDER_ACCOUNT=$(subkey inspect "${PROVIDER}" --output-type json | jq -r '.ss58Address')

    # Populate LATEST_BLOCK with the latest finalized block
    set_latest_block
    START_BLOCK=$((LATEST_BLOCK + 20))
    END_BLOCK=$((START_BLOCK + 10000))

    # Populate PIECE_CID and PIECE_SIZE
    set_piece_vars "$INPUT_FILE" "$INPUT_TMP_FILE"

    # Publish deal
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

    echo "Published deal between "$CLIENT" and "$PROVIDER""
done

echo "All deals published successfully"