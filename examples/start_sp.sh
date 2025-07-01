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

CLIENT="//Alice"
PROVIDER="//Charlie"
P2P_ADDRESS="/ip4/$COLLATOR_IP_ADDR/tcp/62649"
P2P_PUBLIC_KEY="$TMP_PATH/public.pem"
P2P_PRIVATE_KEY="$TMP_PATH/private.pem"
P2P_BOOTSTRAP_PUBLIC_KEY="/tmp/zombienet/charlie-public.pem"
# Config file location
CONFIG="$TMP_PATH/config.toml"
# Deal parameters JSON location
DEAL_PARAMS="$TMP_PATH/deal_params.json"

# Generate ED25519 private key
openssl genpkey -algorithm ED25519 -out "$P2P_PRIVATE_KEY"
# -outpubkey is only available in OpenSSL 3.4.0 onwards
# https://github.com/openssl/openssl/commit/6c03fa21ed4bbc9fd6d3013fdf9f4646d231f831
openssl pkey -in "$P2P_PRIVATE_KEY" -pubout -out "$P2P_PUBLIC_KEY"

# # Generate Peer ID
# P2P_SP_PEER_ID="$(target/release/polka-storage-provider-client generate-peer-id --pubkey "$P2P_PUBLIC_KEY")"
# echo "Generated new peer ID for $PROVIDER: $P2P_SP_PEER_ID"

# Get bootstrap P2P Peer ID. This works after running zombienet locally or in kubernetes
# P2P_BOOTSTRAP_PEER_ID="$(target/release/polka-storage-provider-client generate-peer-id --pubkey "$P2P_BOOTSTRAP_PUBLIC_KEY")"
# echo "Peer ID for bootstrap node: $P2P_BOOTSTRAP_PEER_ID"


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
# rendezvous_point_address = '$P2P_ADDRESS'
# p2p_key = '@$P2P_PRIVATE_KEY'
# rendezvous_point = '$P2P_BOOTSTRAP_PEER_ID'
node_url = 'ws://$COLLATOR_IP_ADDR:42069'
[sealing_configuration]
fill_threshold = 0
wait_deals_delay = '1h'
pre_commit_submission_slack = '1m'" > "$CONFIG"

RUST_LOG="polka_storage_provider_server=debug,tower_http=debug,yamux=off,multistream_select=off,jsonrpsee-client=off,libp2p=debug,storage_proofs_porep=debug,polka_storage_provider_common=debug" target/release/polka-storage-provider-server \
    --sr25519-key "$PROVIDER" \
    --config "$CONFIG"
