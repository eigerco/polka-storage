#!/usr/bin/env bash
set -e

# This script will publish a file to Alice, Bob and Charlie
# it is supposed to be used in tandem with start_sps.sh
#
# It only publishes a single deal to each but changing is simple
# simply change the `seq` call in the `upload_file` function
#
# To retrieve the file from the multiple providers at once you can run:
#
# RUST_LOG=debug cargo r -r -p polka-fetch -- \
#   --provider /ip4/127.0.0.1/tcp/47002 \
#   --provider /ip4/127.0.0.1/tcp/46002 \
#   --provider /ip4/127.0.0.1/tcp/45002 \
# --output /tmp/t \
# --overwrite \
# --payload-cid bafybeiefli7iugocosgirzpny4t6yxw5zehy6khtao3d252pbf352xzx5q
#
# The CID is for the spaceglenda.jpg file

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

CLIENT="//Eve"

INPUT_FILE="$1"
INPUT_FILE_NAME="$(basename "$INPUT_FILE")"
INPUT_TMP_FILE="/tmp/$INPUT_FILE_NAME.car"

target/release/mater-cli convert -q --overwrite "$INPUT_FILE" "$INPUT_TMP_FILE" &&
INPUT_COMMP="$(target/release/polka-storage-provider-client proofs commp "$INPUT_TMP_FILE")"
PIECE_CID="$(echo "$INPUT_COMMP" | jq -r ".cid")"
PIECE_SIZE="$(echo "$INPUT_COMMP" | jq ".size")"

function base_port {
    case $1 in
        "//Alice")
            echo 45000
            ;;
        "//Bob")
            echo 46000
            ;;
        "//Charlie")
            echo 47000
            ;;
        *)
            echo "unknown account"
            exit 1
            ;;
    esac
}

function public_key {
    case $1 in
        "//Alice")
            echo 5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY
            ;;
        "//Bob")
            echo 5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty
            ;;
        "//Charlie")
            echo 5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y
            ;;
        "//Dave")
            echo 5DAAnrj7VHTznn2AWBemMuyBwZWs6FNFjdyVXUeYum3PTXFy
            ;;
        "//Eve")
            echo 5HGjWAeFDfFCWPsjFQdVV2Msvz2XtMktvgocEZcCj68kUMaw
            ;;
        "//Ferdie")
            echo 5CiPPseXPECbkjWCa6MnjNokrgYjMqmKndv2rSnekmSK2DjL
            ;;
        *)
            echo "unknown account"
            exit 1
            ;;
    esac
}

function upload_file {
    ACCOUNT=$1
    STORAGE_URL=$2
    RPC_URL=$3

    for i in $(seq 60 60 | tac);
    do
        DEAL_JSON=$(
            jq -n \
        --arg piece_cid "$PIECE_CID" \
        --argjson piece_size "$PIECE_SIZE" \
        --arg client_addr "$(public_key "$CLIENT")" \
        --arg provider_addr "$(public_key "$ACCOUNT")" \
        --argjson start_block "$i" \
        '{
                "piece_cid": $piece_cid,
                "piece_size": $piece_size,
                "client": $client_addr,
                "provider": $provider_addr,
                "label": "",
                "start_block": $start_block,
                "end_block": 250,
                "storage_price_per_block": 500,
                "provider_collateral": 1250,
                "state": "Published"
            }'
        )
        SIGNED_DEAL_JSON="$(RUST_LOG=error target/release/polka-storage-provider-client sign-deal --sr25519-key "$CLIENT" "$DEAL_JSON")"

        DEAL_CID="$(RUST_LOG=error target/release/polka-storage-provider-client propose-deal --rpc-server-url "$RPC_URL" "$DEAL_JSON")"
        echo "-------------------------- Uploading deal $i..."
        echo
        curl -X PUT -F "upload=@$INPUT_FILE" "$STORAGE_URL/upload/$DEAL_CID"

        echo
        echo "-------------------------- Publishing deal $i..."
        target/release/polka-storage-provider-client publish-deal --rpc-server-url "$RPC_URL" "$SIGNED_DEAL_JSON"
    done
}

declare -a ACCOUNTS=("//Alice" "//Bob" "//Charlie")
for ACCOUNT in "${ACCOUNTS[@]}"; do
    STORAGE_URL="http://127.0.0.1:$(base_port "$ACCOUNT")"
    RPC_URL="http://127.0.0.1:$(echo "$(base_port "$ACCOUNT") + 1" | bc)"
    # RETRIEVAL_URL="/ip4/127.0.0.1/tcp/$(echo "$(base_port "$ACCOUNT") + 2" | bc)"

    for i in $(seq 60 60 | tac);
    do
        upload_file "$ACCOUNT" "$STORAGE_URL" "$RPC_URL" &
    done
done

# wait until user Ctrl+Cs so that the commitment can actually be calculated
wait
