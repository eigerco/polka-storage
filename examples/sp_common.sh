#!/usr/bin/env bash
set -e

write_deal_parameters() {
    local DEAL_PARAMS_PATH="$1"
    local MINIMUM_PRICE_PER_BLOCK="${2:-200}"
    local LOWER_DEAL_DURATION="${3:-50}"
    local UPPER_DEAL_DURATION="${4:-5256000}"

    echo "{ \"minimum_price_per_block\": $MINIMUM_PRICE_PER_BLOCK, \"deal_duration\": { \"lower\": $LOWER_DEAL_DURATION, \"upper\": $UPPER_DEAL_DURATION } }" > "$DEAL_PARAMS_PATH"
}

register_storage_provider() {
    local SP_NAME="$1"
    local PORT="$2"
    local DEAL_PARAMS_PATH="$3"
    local COLLATOR_WS_ADDR="${4:-"ws://127.0.0.1:42069"}"

    SP_MULTIADDR="/ip4/127.0.0.1/tcp/$PORT"

    echo "Registering storage provider $SP_NAME..."
    RUST_LOG='debug,jsonrpsee-client=off' target/release/storagext-cli \
        --sr25519-key "$SP_NAME" \
        --node-rpc "$COLLATOR_WS_ADDR" \
        storage-provider register \
        --post-proof "8MiB" \
        --deal-parameters @"$DEAL_PARAMS_PATH" \
        "$SP_MULTIADDR"
}

generate_config_file() {
    local CONFIG_FILE_PATH="$1"
    local PORT="$2"
    local DB_DIR="$3"
    local STORAGE_DIR="$4"
    local WAIT_DEALS_DELAY="${5:-1h}"
    local COLLATOR_WS_ADDR="${6:-"ws://127.0.0.1:42069"}"

    {
        echo "seal_proof = '8MiB'"
        echo "post_proof = '8MiB'"
        echo "porep_parameters = 'target/params/8MiB.porep.params'"
        echo "post_parameters = 'target/params/8MiB.post.params'"
        echo "node_url = '$COLLATOR_WS_ADDR'"
        echo "upload_listen_address = '127.0.0.1:$PORT'"

        if [ -n "$DB_DIR" ]; then
            echo "database_directory = '$DB_DIR'"
        fi

        if [ -n "$STORAGE_DIR" ]; then
            echo "storage_directory = '$STORAGE_DIR'"
        fi

        echo "[sealing_configuration]"
        echo "fill_threshold = 0"
        echo "wait_deals_delay = '$WAIT_DEALS_DELAY'"
        echo "pre_commit_submission_slack = '1m'"
    } > "$CONFIG_FILE_PATH"
}

start_storage_provider() {
    local SP_NAME="$1"
    local CONFIG_FILE="$2"

    echo "Starting storage provider $SP_NAME with config $CONFIG_FILE..."
    RUST_LOG="tower_http=debug,polka_storage_provider_server=debug,yamux=off,multistream_select=off,jsonrpsee-client=off,libp2p=debug,storage_proofs_porep=debug,polka_storage_provider_common=debug" \
        target/release/polka-storage-provider-server \
        --sr25519-key "$SP_NAME" \
        --config "$CONFIG_FILE"
}
