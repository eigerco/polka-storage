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
source "$(dirname "$0")/deal_common.sh"

CLIENT="//Alice"
PROVIDER="//Charlie"

INPUT_FILE="$1"
INPUT_FILE_NAME="$(basename "$INPUT_FILE")"
INPUT_TMP_FILE="/tmp/$INPUT_FILE_NAME.car"

for i in $(seq 50 60);
do
    set_piece_vars "$INPUT_FILE" "$INPUT_TMP_FILE"
    set_latest_block

    START_BLOCK=$((i + $LATEST_BLOCK))
    END_BLOCK=$((i + $LATEST_BLOCK + 100))
    publish_deal \
        "$CLIENT" \
        "$PROVIDER" \
        "$PIECE_CID" \
        $PIECE_SIZE \
        $START_BLOCK \
        $END_BLOCK \
        "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY" \
        "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y" \
        "$INPUT_FILE"
    echo "Published deal between ${PROVIDER} and ${CLIENT}"
done

echo 'Done publishing deals'
