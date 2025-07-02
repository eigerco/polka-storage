#!/usr/bin/env bash
set -e
set -x

trap "trap - SIGTERM && kill -- -$$" SIGINT SIGTERM EXIT

# requires the testnet to be running!
export DISABLE_XT_WAIT_WARNING=1
TMPDIR="${TMPDIR:-/tmp}"
TMP_PATH="$TMPDIR/polka-storage-provider"

mkdir -p "$TMP_PATH"

COLLATOR_IP_ADDR="127.0.0.1"

PROVIDER="//Charlie"
# Config file location
CONFIG="$TMP_PATH/config.toml"
# Deal parameters JSON location
DEAL_PARAMS="$TMP_PATH/deal_params.json"


RUST_LOG='debug,jsonrpsee-client=off' target/release/storagext-cli \
    --sr25519-key "//Charlie" \
    --node-rpc "ws://$COLLATOR_IP_ADDR:42069" \
    storage-provider register \
    --post-proof "8MiB" \
    "/ip4/127.0.0.1/tcp/8001"
wait

echo '{ "minimum_price_per_block": 200, "deal_duration": { "lower": 50, "upper": 5256000 }}' > "$DEAL_PARAMS"
# Setup deal parameters, has to go after registration.
RUST_LOG='debug,jsonrpsee-client=off' target/release/storagext-cli \
    --node-rpc "ws://$COLLATOR_IP_ADDR:42069" \
    --sr25519-key "$PROVIDER" \
    market publish-deal-parameters \
    --deal-parameters @"$DEAL_PARAMS"

echo "seal_proof = '8MiB'
post_proof = '8MiB'
porep_parameters = 'target/params/8MiB.porep.params'
post_parameters = 'target/params/8MiB.post.params'
node_url = 'ws://$COLLATOR_IP_ADDR:42069'
[sealing_configuration]
fill_threshold = 0
wait_deals_delay = '1h'
pre_commit_submission_slack = '1m'" > "$CONFIG"

RUST_LOG="polka_storage_provider_server=debug,tower_http=debug,yamux=off,multistream_select=off,jsonrpsee-client=off,libp2p=debug,storage_proofs_porep=debug,polka_storage_provider_common=debug" target/release/polka-storage-provider-server \
    --sr25519-key "$PROVIDER" \
    --config "$CONFIG"
