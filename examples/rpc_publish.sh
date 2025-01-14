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
P2P_PUBLIC_KEY="/tmp/public.pem"
P2P_PRIVATE_KEY="/tmp/private.pem"
P2P_ADDRESS="/ip4/127.0.0.1/tcp/62649"

# Generate ED25519 private key
openssl genpkey -algorithm ED25519 -out "$P2P_PRIVATE_KEY" -outpubkey "$P2P_PUBLIC_KEY"
target/release/mater-cli convert -q --overwrite "$INPUT_FILE" "$INPUT_TMP_FILE" &&
INPUT_COMMP="$(target/release/polka-storage-provider-client proofs commp "$INPUT_TMP_FILE")"
PIECE_CID="$(echo "$INPUT_COMMP" | jq -r ".cid")"
PIECE_SIZE="$(echo "$INPUT_COMMP" | jq ".size")"
PEER_ID="$(target/release/polka-storage-provider-client generate-peer-id --pubkey "$P2P_PUBLIC_KEY")"
P2P_CONFIG="/tmp/bootstrap.toml"
echo "address = '$P2P_ADDRESS'
keypair = '@$P2P_PRIVATE_KEY'
" > "$P2P_CONFIG"


# Setup balances
target/release/storagext-cli --sr25519-key "$CLIENT" market add-balance 250000000000 &
target/release/storagext-cli --sr25519-key "$PROVIDER" market add-balance 250000000000 &
# We can process a transaction by charlie and alice, but we can't in the same transaction
# register one of them as the storage provider
wait

# It's a test setup based on the local verifying keys, everyone can run those extrinsics currently.
# Each of the keys is different, because the processes are running in parallel.
# If they were running in parallel on the same account, they'd conflict with each other on the transaction nonce.
target/release/storagext-cli --sr25519-key "//Charlie" storage-provider register "$PEER_ID" &
target/release/storagext-cli --sr25519-key "//Alice" proofs set-porep-verifying-key @2KiB.porep.vk.scale &
target/release/storagext-cli --sr25519-key "//Bob" proofs set-post-verifying-key @2KiB.post.vk.scale &

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
        "provider_collateral": 1250,
        "state": "Published"
    }'
)
SIGNED_DEAL_JSON="$(RUST_LOG=error target/release/polka-storage-provider-client sign-deal --sr25519-key "$CLIENT" "$DEAL_JSON")"

(RUST_LOG=debug target/release/polka-storage-provider-server --sr25519-key "$PROVIDER" --seal-proof "2KiB" --post-proof "2KiB" --porep-parameters 2KiB.porep.params --post-parameters 2KiB.post.params --p2p-config "$P2P_CONFIG") &
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
