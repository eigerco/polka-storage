#!/usr/bin/env bash
# set -e

trap "trap - SIGTERM && kill -- -$$" SIGINT SIGTERM EXIT

# requires the testnet to be running!
export DISABLE_XT_WAIT_WARNING=1

CLIENT="//Alice"
PROVIDER="//Charlie"

SCRIPT_CAR="/tmp/$(basename "$0").car"
SCRIPT_CID="$(target/release/mater-cli convert -q --overwrite "$0" "$SCRIPT_CAR")"

echo "$SCRIPT_CAR" "$SCRIPT_CID"

# Setup balances
target/release/storagext-cli --sr25519-key "$CLIENT" market add-balance 250000000000 &
target/release/storagext-cli --sr25519-key "$PROVIDER" market add-balance 250000000000 &
wait

# Register the SP
target/release/storagext-cli --sr25519-key "//Charlie" storage-provider register "peer_id"

(RUST_LOG=trace target/release/polka-storage-provider rpc server --sr25519-key "$PROVIDER") &
sleep 5

DEAL_JSON=$(
    jq -n \
   --arg piece_cid "$SCRIPT_CID" \
   --argjson piece_size "$(stat --printf="%s" "$SCRIPT_CAR")" \
   '{
        "piece_cid": $piece_cid,
        "piece_size": $piece_size,
        "client": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
        "provider": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
        "label": "",
        "start_block": 100000,
        "end_block": 100100,
        "storage_price_per_block": 500,
        "provider_collateral": 1250,
        "state": "Published"
    }'
)
echo "$DEAL_JSON"

DEAL_CID="$(target/release/polka-storage-provider rpc client propose-deal "$DEAL_JSON")"
echo "$DEAL_CID"

echo "$SCRIPT_CAR"
curl --upload-file "$SCRIPT_CAR" "http://localhost:8001/upload/$DEAL_CID"

SIGNED_DEAL_JSON="$(target/release/polka-storage-provider rpc client sign-deal --sr25519-key "$CLIENT" "$DEAL_JSON")"
echo "$SIGNED_DEAL_JSON"

target/release/polka-storage-provider rpc client publish-deal "$SIGNED_DEAL_JSON"
echo "finished"
