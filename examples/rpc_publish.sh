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

trap "trap - SIGTERM && kill -- -$$" SIGINT SIGTERM

# requires the testnet to be running!
export DISABLE_XT_WAIT_WARNING=1
TMPDIR="${TMPDIR:-/tmp}"
TMP_PATH="$TMPDIR/polka-storage-provider"

mkdir -p "$TMP_PATH"

CLIENT="//Alice"
PROVIDER="//Charlie"

INPUT_FILE="$1"
INPUT_FILE_NAME="$(basename "$INPUT_FILE")"
# CARv2 file location
INPUT_TMP_FILE="$TMP_PATH/$INPUT_FILE_NAME.car"

source "$(dirname "$0")/deal_common.sh"

set_latest_block

START_BLOCK=$((LATEST_BLOCK + 20))
END_BLOCK=$((LATEST_BLOCK + 100))

set_piece_vars "$INPUT_FILE" "$INPUT_TMP_FILE"

publish_deal \
    "$CLIENT" \
    "$PROVIDER" \
    "$PIECE_CID" \
    $PIECE_SIZE \
    $START_BLOCK \
    $END_BLOCK \
    $(subkey inspect "${CLIENT}" | awk -F': +' '/SS58 Address/ {print $2}') \
    $(subkey inspect "${PROVIDER}" | awk -F': +' '/SS58 Address/ {print $2}') \
    "$INPUT_FILE"

echo "Storage deal successfully published!"
