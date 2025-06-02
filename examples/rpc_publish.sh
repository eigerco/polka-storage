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
TMPDIR="${TMPDIR:-/tmp}"
TMP_PATH="$TMPDIR/polka-storage"

mkdir -p "$TMP_PATH"

CLIENT="//Alice"
PROVIDER="//Charlie"

INPUT_FILE="$1"
INPUT_FILE_NAME="$(basename "$INPUT_FILE")"
# CARv2 file location
INPUT_TMP_FILE="$TMP_PATH/$INPUT_FILE_NAME.car"
# Config file location
CONFIG="$TMP_PATH/config.toml"
# P2P Node variables
P2P_PUBLIC_KEY="$TMP_PATH/public.pem"
P2P_PRIVATE_KEY="$TMP_PATH/private.pem"
P2P_BOOTSTRAP_PUBLIC_KEY="/tmp/zombienet/charlie-public.pem"
P2P_ADDRESS="/ip4/127.0.0.1/tcp/62649"
# Deal parameters JSON location
DEAL_PARAMS="$TMP_PATH/deal_params.json"

# Generate ED25519 private key
openssl genpkey -algorithm ED25519 -out "$P2P_PRIVATE_KEY"
# -outpubkey is only available in OpenSSL 3.4.0 onwards
# https://github.com/openssl/openssl/commit/6c03fa21ed4bbc9fd6d3013fdf9f4646d231f831
openssl pkey -in "$P2P_PRIVATE_KEY" -pubout -out "$P2P_PUBLIC_KEY"

# Generate Peer ID
P2P_BOOTSTRAP_PEER_ID="$(target/release/polka-storage-provider-client generate-peer-id --pubkey "$P2P_BOOTSTRAP_PUBLIC_KEY")"

# Convert file to CARv2 format
target/release/mater-cli convert -q --overwrite "$INPUT_FILE" "$INPUT_TMP_FILE" &&

# Calculate COMMP and set PIECE_CID and PIECE_SIZE
INPUT_COMMP="$(target/release/polka-storage-provider-client proofs commp "$INPUT_TMP_FILE")"
PIECE_CID="$(echo "$INPUT_COMMP" | jq -r ".cid")"
PIECE_SIZE="$(echo "$INPUT_COMMP" | jq ".size")"

# Generate Peer ID from public key
PEER_ID="$(target/release/polka-storage-provider-client generate-peer-id --pubkey "$P2P_PUBLIC_KEY")"

# echo config file in the file in the /tmp folder
echo "seal_proof = '8MiB'
post_proof = '8MiB'
porep_parameters = 'target/porep_params_8MiB'
post_parameters = 'target/post_params_8MiB'
rendezvous_point_address = '$P2P_ADDRESS'
p2p_key = '@$P2P_PRIVATE_KEY'
rendezvous_point = '$P2P_BOOTSTRAP_PEER_ID'" > "$CONFIG"

# echo deal parameters in the file in the /tmp folder
echo '{ "minimum_price_per_block": 200, "deal_duration": { "lower": 50, "upper": 1800 }}' > "$DEAL_PARAMS"


# It's a test setup based on the local verifying keys, everyone can run those extrinsics currently.
# Each of the keys is different, because the processes are running in parallel.
# If they were running in parallel on the same account, they'd conflict with each other on the transaction nonce.
target/release/storagext-cli --sr25519-key "//Charlie" storage-provider register "$PEER_ID"

wait

# Setup deal parameters, has to go after registration.
RUST_LOG=debug target/release/storagext-cli --sr25519-key "$PROVIDER" market publish-deal-parameters \
    --deal-parameters @"$DEAL_PARAMS" &
wait

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
        "start_block": 200,
        "end_block": 250,
        "storage_price_per_block": 500,
        "state": "Published"
    }'
)
SIGNED_DEAL_JSON="$(RUST_LOG=error target/release/polka-storage-provider-client sign-deal --sr25519-key "$CLIENT" "$DEAL_JSON")"

(RUST_LOG=debug target/release/polka-storage-provider-server --sr25519-key "$PROVIDER" --config "$CONFIG") &
sleep 5 # gives time for the server to start

DEAL_CID="$(RUST_LOG=error target/release/polka-storage-provider-client propose-deal "$DEAL_JSON")"
echo "$DEAL_CID"

# Regular upload
# curl --upload-file "$INPUT_FILE" "http://localhost:8001/upload/$DEAL_CID"

# Multipart upload
curl -X PUT -F "upload=@$INPUT_FILE" "http://localhost:8001/upload/$DEAL_CID"

target/release/polka-storage-provider-client publish-deal "$SIGNED_DEAL_JSON"

# wait until user Ctrl+Cs so that the commitment can actually be calculated
wait
