#!/usr/bin/env bash
set -e

# Starts a single Storage Provider Charlie

trap "trap - SIGTERM && kill -- -$$" SIGINT SIGTERM

source "$(dirname "$0")/sp_common.sh"

export DISABLE_XT_WAIT_WARNING=1

# CONFIGURATION
TMPDIR="${TMPDIR:-/tmp}"
TMP_PATH="$TMPDIR/polka-storage-provider"
PROVIDER="//Charlie"
PORT="8001"
CONFIG="$TMP_PATH/config.toml"
DEAL_PARAMS="$TMP_PATH/deal_params.json"

# Ensure top-level TMP path exists
mkdir -p "$TMP_PATH"

# Write deal parameters
write_deal_parameters "$DEAL_PARAMS"
# Register Storage Provider
register_storage_provider "$PROVIDER" "$PORT" "$DEAL_PARAMS"
wait

# Write config
generate_config_file "$CONFIG" "$PORT"
# Start Storage Provider
start_storage_provider "$PROVIDER" "$CONFIG"
