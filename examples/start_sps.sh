#!/usr/bin/env bash
set -e

# Starts 3 storage providers — Alice, Bob & Charlie
# You can check the configuration in their respective folders

trap "trap - SIGTERM && kill -- -$$" SIGINT SIGTERM EXIT

# requires the testnet to be running!
export DISABLE_XT_WAIT_WARNING=1

TMPDIR="${TMPDIR:-/tmp}"
TMP_PATH="$TMPDIR/polka-storage-provider"

mkdir -p "$TMP_PATH"

COLLATOR_IP_ADDR="127.0.0.1"
DEAL_PARAMS="$TMP_PATH/deal_params.json"

function register_storage_provider {
    local SP_NAME="$1"
    local SP_MULTIADDR_PORT="$2"
    local SP_TMP_DIR="$TMP_PATH/${SP_NAME#//}"

    mkdir -p "$SP_TMP_DIR"

    RUST_LOG="storagext=debug,storagext-cli=debug" target/release/storagext-cli \
        --sr25519-key "$SP_NAME" \
        --node-rpc "ws://$COLLATOR_IP_ADDR:42069" \
        storage-provider register \
        --post-proof "8MiB" \
        --deal-parameters @"$DEAL_PARAMS" \
        "/ip4/127.0.0.1/tcp/$SP_MULTIADDR_PORT"
}

function sp_tmp_dir {
    echo "$TMP_PATH/$1"
}

function main {
    echo '{ "minimum_price_per_block": 200, "deal_duration": { "lower": 50, "upper": 5256000 }}' > "$DEAL_PARAMS"
    declare -a ACCOUNTS=("//Alice" "//Bob" "//Charlie")
    declare -a PORTS=("8001" "8002" "8003")
    for i in "${!ACCOUNTS[@]}"; do
        ACCOUNT="${ACCOUNTS[$i]}"
        PORT="${PORTS[$i]}"
        register_storage_provider "$ACCOUNT" "$PORT"
    done
    wait

    for i in "${!ACCOUNTS[@]}"; do
        ACCOUNT="${ACCOUNTS[$i]}"
        PORT="${PORTS[$i]}"
        ACCOUNT_DIR="$(sp_tmp_dir "$ACCOUNT#//")"
        DB_DIR="$(sp_tmp_dir "$ACCOUNT#//")/db"
        STORAGE_DIR="$(sp_tmp_dir "$ACCOUNT#//")/storage"

        mkdir -p "$DB_DIR"
        mkdir -p "$STORAGE_DIR"
        echo "
            seal_proof = '8MiB'
            post_proof = '8MiB'
            porep_parameters = 'target/params/8MiB.porep.params'
            post_parameters = 'target/params/8MiB.post.params'
            node_url = 'ws://$COLLATOR_IP_ADDR:42069'
            upload_listen_address = '127.0.0.1:$PORT'
            database_directory = '$DB_DIR'
            storage_directory = '$STORAGE_DIR'
            [sealing_configuration]
            fill_threshold = 0
            wait_deals_delay = '5m'
            pre_commit_submission_slack = '1m'" | sed 's/\s\+//' > "$ACCOUNT_DIR/config.toml"

        echo "Starting storage provider "$PROVIDER" with upload address 127.0.0.1:"$PORT"..."
        RUST_LOG="tower_http=debug,polka_storage_provider_server=debug,polka_storage_provider_server::p2p=trace,yamux=off,multistream_select=off,polka_storage_provider_common=debug" target/release/polka-storage-provider-server \
            --sr25519-key "$ACCOUNT" \
            --config "$ACCOUNT_DIR/config.toml" &
    done
    wait
}

main
