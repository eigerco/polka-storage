#!/usr/bin/env bash
set -e

# Starts 3 Storage Providers — Alice, Bob & Charlie

trap "trap - SIGTERM && kill -- -$$" SIGINT SIGTERM EXIT

source "$(dirname "$0")/sp_common.sh"

# requires the testnet to be running!
export DISABLE_XT_WAIT_WARNING=1

# CONFIGURATION
TMPDIR="${TMPDIR:-/tmp}"
TMP_PATH="$TMPDIR/polka-storage-provider"
DEAL_PARAMS="$TMP_PATH/deal_params.json"
WAIT_DEALS_DELAY="5m"

ACCOUNTS=("//Alice" "//Bob" "//Charlie")
PORTS=("8001" "8002" "8003")

# Ensure top-level TMP path exists
mkdir -p "$TMP_PATH"

# Write deal parameters
write_deal_parameters "$DEAL_PARAMS"

# Register each Storage Provider
for i in "${!ACCOUNTS[@]}"; do
    SP_NAME="${ACCOUNTS[$i]}"
    SP_PORT="${PORTS[$i]}"

    register_storage_provider "$SP_NAME" "$SP_PORT" "$DEAL_PARAMS"
done

wait

for i in "${!ACCOUNTS[@]}"; do
    SP_NAME="${ACCOUNTS[$i]}"
    SP_PORT="${PORTS[$i]}"
    SP_TMP_DIR="$TMP_PATH/${SP_NAME#//}"

    DB_DIR="$SP_TMP_DIR/db"
    STORAGE_DIR="$SP_TMP_DIR/storage"
    CONFIG_FILE="$SP_TMP_DIR/config.toml"

    # Ensure DB and Storage path exist
    mkdir -p "$DB_DIR" "$STORAGE_DIR"

    # Generate config file
    generate_config_file "$CONFIG_FILE" "$SP_PORT" "$DB_DIR" "$STORAGE_DIR" "$WAIT_DEALS_DELAY"

    # Start Storage Provider
    start_storage_provider "$SP_NAME" "$CONFIG_FILE" &
done

wait
