#!/usr/bin/env bash
set -e

# Starts 3 storage providers — Alice, Bob & Charlie
# Ports start at 45000 and increment 1000 for each, so 45000, 46000 & 47000
# You can check the configuration in their respective folders
#
# It also sets up balances for all accounts — Alice, Bob, Charlie, Dave, Eve & Ferdie

trap "trap - SIGTERM && kill -- -$$" SIGINT SIGTERM EXIT

# requires the testnet to be running!
export DISABLE_XT_WAIT_WARNING=1

mkdir -p /tmp/polka-storage-provider

P2P_BOOTSTRAP_ADDRESS="/ip4/127.0.0.1/tcp/62649"
P2P_BOOTSTRAP_PUBLIC_KEY="/tmp/zombienet/charlie-public.pem"

# Adds funds to all test accounts
function setup_balances {
    declare -a ACCOUNTS=("//Alice" "//Bob" "//Charlie" "//Dave" "//Eve" "//Ferdie")
    for ACCOUNT in "${ACCOUNTS[@]}"; do
        RUST_LOG="storagext=debug,storagext-cli=debug" target/release/storagext-cli --sr25519-key "$ACCOUNT" market add-balance 250000000000 &
    done
    wait
}

function setup_network_keys {
    # It's a test setup based on the local verifying keys, everyone can run those extrinsics currently.
    # Each of the keys is different, because the processes are running in parallel.
    # If they were running in parallel on the same account, they'd conflict with each other on the transaction nonce.
    RUST_LOG="storagext=debug,storagext-cli=debug" target/release/storagext-cli --sr25519-key "//Alice" proofs set-porep-verifying-key --registered-proof 8MiB @8MiB.porep.vk.scale &
    RUST_LOG="storagext=debug,storagext-cli=debug" target/release/storagext-cli --sr25519-key "//Bob" proofs set-post-verifying-key --registered-proof 8MiB @8MiB.post.vk.scale &
    wait
}

function register_storage_provider {
    local SP_NAME="$1"
    local P2P_SP_KEY_DIR="/tmp/polka-storage-provider/$SP_NAME"
    local P2P_PUBLIC_KEY="$P2P_SP_KEY_DIR/public.pem"
    local P2P_PRIVATE_KEY="$P2P_SP_KEY_DIR/private.pem"

    mkdir -p "$P2P_SP_KEY_DIR"

    # Generate ED25519 private key
    openssl genpkey -algorithm ED25519 -out "$P2P_PRIVATE_KEY"
    # -outpubkey is only available in OpenSSL 3.4.0 onwards
    # https://github.com/openssl/openssl/commit/6c03fa21ed4bbc9fd6d3013fdf9f4646d231f831
    openssl pkey -in "$P2P_PRIVATE_KEY" -pubout -out "$P2P_PUBLIC_KEY"

    # Generate Peer ID
    P2P_SP_PEER_ID="$(target/release/polka-storage-provider-client generate-peer-id --pubkey "$P2P_PUBLIC_KEY")"
    echo "Generated new peer ID for $PROVIDER: $P2P_SP_PEER_ID"

    # Get bootstrap P2P Peer ID. This works after running zombienet locally or in kubernetes
    P2P_BOOTSTRAP_PEER_ID="$(target/release/polka-storage-provider-client generate-peer-id --pubkey "$P2P_BOOTSTRAP_PUBLIC_KEY")"
    echo "Peer ID for bootstrap node: $P2P_BOOTSTRAP_PEER_ID"

    RUST_LOG="storagext=debug,storagext-cli=debug" target/release/storagext-cli --sr25519-key "$SP_NAME" storage-provider register --post-proof "8MiB" "$P2P_SP_PEER_ID"
}

function sp_key_dir {
    echo "/tmp/polka-storage-provider/$1"
}

function sp_public_key {
    echo "$(sp_key_dir "$1")/public.pem"
}

function sp_private_key {
    echo "$(sp_key_dir "$1")/private.pem"
}

function ports {
    case $1 in
        "//Alice")
            local PORT=45000
            echo "upload_listen_address = '127.0.0.1:$PORT'
                  rpc_listen_address = '127.0.0.1:$(echo "$PORT + 1" | bc)'
                  retrieval_listen_address = '/ip4/127.0.0.1/tcp/$(echo "$PORT + 2" | bc)'" |
            sed "s/\s\+//"
            ;;
        "//Bob")
            local PORT=46000
            echo "upload_listen_address = '127.0.0.1:$PORT'
                  rpc_listen_address = '127.0.0.1:$(echo "$PORT + 1" | bc)'
                  retrieval_listen_address = '/ip4/127.0.0.1/tcp/$(echo "$PORT + 2" | bc)'" |
            sed "s/\s\+//"
            ;;
        "//Charlie")
            local PORT=47000
            echo "upload_listen_address = '127.0.0.1:$PORT'
                  rpc_listen_address = '127.0.0.1:$(echo "$PORT + 1" | bc)'
                  retrieval_listen_address = '/ip4/127.0.0.1/tcp/$(echo "$PORT + 2" | bc)'" |
            sed "s/\s\+//"
            ;;
    esac
}

setup_balances
setup_network_keys

declare -a ACCOUNTS=("//Alice" "//Bob" "//Charlie")
for ACCOUNT in "${ACCOUNTS[@]}"; do
    register_storage_provider "$ACCOUNT"
done
wait

# Get bootstrap P2P Peer ID. This works after running zombienet locally or in kubernetes
P2P_BOOTSTRAP_PEER_ID="$(target/release/polka-storage-provider-client generate-peer-id --pubkey "$P2P_BOOTSTRAP_PUBLIC_KEY")"
echo "Peer ID for bootstrap node: $P2P_BOOTSTRAP_PEER_ID"

declare -a ACCOUNTS=("//Alice" "//Bob" "//Charlie")
for ACCOUNT in "${ACCOUNTS[@]}"; do
    echo "
        $(ports "$ACCOUNT")
        seal_proof = '8MiB'
        post_proof = '8MiB'
        porep_parameters = '8MiB.porep.params'
        post_parameters = '8MiB.post.params'
        p2p_key = '@$(sp_private_key "$ACCOUNT")'
        rendezvous_point_address = '$P2P_BOOTSTRAP_ADDRESS'
        rendezvous_point = '$P2P_BOOTSTRAP_PEER_ID'
        [sealing_configuration]
        fill_threshold = 0
        wait_deals_delay = '5m'
        pre_commit_submission_slack = '1m'" | sed 's/\s\+//' > "$(sp_key_dir "$ACCOUNT")/config.toml"

    RUST_LOG="polka_storage_provider_server=debug" target/release/polka-storage-provider-server \
        --sr25519-key "$ACCOUNT" \
        --config "$(sp_key_dir "$ACCOUNT")/config.toml" &
done
wait
