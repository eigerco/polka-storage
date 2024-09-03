#!/usr/bin/env bash
set -e

if ! command -v storagext-cli >/dev/null 2>&1; then
    echo "Make sure to follow https://eigerco.github.io/polka-storage-book/getting-started/local-testnet.html#native-binaries."
    echo "This script relies on having a fresh testnet running and 'storagext-cli' in the PATH."
    exit 1
fi

# Execute command with the descrption
execute() {
    # Print description
    echo "-- $1 --"
    echo "Command: $2"

    # Execute command and print result
    result=$(eval "$2")

    echo "Result: $result"
    echo
}

startup_validate() {
    execute 'Wait until the chain starts' "storagext-cli system wait-for-height 1"
    height=$(storagext-cli system get-height | awk '{print $3}')
    if [[ $height -ne 1 ]]; then
        echo "For this script to work, it needs to be run exactly at the second block. Current: $height"
        exit 0
    fi
}

startup_validate

HUSKY_DEAL='[
    {

        "piece_cid": "bafybeihxgc67fwhdoxo2klvmsetswdmwwz3brpwwl76qizbsl6ypro6vxq",
        "piece_size": 1278,
        "client": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
        "provider": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
        "label": "My lovely Husky (husky.jpg)",
        "start_block": 65,
        "end_block": 115,
        "storage_price_per_block": 500000000,
        "provider_collateral": 12500000000,
        "state": "Published"

    }
]'
echo "$HUSKY_DEAL" > husky-deal.json

PRE_COMMIT_HUSKY='{
    "sector_number": 1,
    "sealed_cid": "bafk2bzaceajreoxfdcpdvitpvxm7vkpvcimlob5ejebqgqidjkz4qoug4q6zu",
    "deal_ids": [0],
    "expiration": 165,
    "unsealed_cid": "bafk2bzaceajreoxfdcpdvitpvxm7vkpvcimlob5ejebqgqidjkz4qoug4q6zu",
    "seal_proof": "StackedDRG2KiBV1P1"

}'
echo "$PRE_COMMIT_HUSKY" > pre-commit-husky.json

PROVE_COMMIT_HUSKY='{
    "sector_number": 1,
    "proof": "beef"
}'
echo "$PROVE_COMMIT_HUSKY" > prove-commit-husky.json

WINDOWED_POST='{
    "deadline": 0,
    "partitions": [0],
    "proof": {
        "post_proof": "2KiB",
        "proof_bytes": "beef"
    }
}'
echo "$WINDOWED_POST" >windowed-post.json

FAULT_DECLARATION='[
    {
        "deadline": 0,
        "partition": 0,
        "sectors": [1]
    }
]

'
echo "$FAULT_DECLARATION" >fault-declaration.json

PROVING_PERIOD_START=61
FIRST_DEADLINE_END=81
SECOND_DEADLINE_START=101
DEAL_ID=0
DEAL_END=115

execute "Registering Charlie as a storage provider" 'storagext-cli --sr25519-key "//Charlie" storage-provider register Charlie'
execute 'Adding balance to Alice`s account' 'storagext-cli --sr25519-key "//Alice" market add-balance 25000000000'
execute 'Adding balance to Charlie`s account' 'storagext-cli --sr25519-key "//Charlie" market add-balance 12500000000'
execute 'Publishing a storage deal' 'storagext-cli --sr25519-key  "//Charlie" market publish-storage-deals --client-sr25519-key  "//Alice" "@husky-deal.json"'
execute 'Pre-commit a sector' 'storagext-cli --sr25519-key "//Charlie" storage-provider pre-commit "@pre-commit-husky.json"'
execute 'Prove committed sector' 'storagext-cli --sr25519-key "//Charlie" storage-provider prove-commit "@prove-commit-husky.json"'

execute 'Wait until the proving period starts' "storagext-cli system wait-for-height $PROVING_PERIOD_START"
execute 'Submitting windowed post' 'storagext-cli --sr25519-key "//Charlie" storage-provider submit-windowed-post "@windowed-post.json"'

execute 'Wait until the first deadline passes' "storagext-cli system wait-for-height $FIRST_DEADLINE_END"
execute 'Submit fault declaration for the sector' 'storagext-cli --sr25519-key "//Charlie" storage-provider declare-faults "@fault-declaration.json"'
execute 'Declare faults recovered' 'storagext-cli --sr25519-key "//Charlie" storage-provider declare-faults-recovered "@fault-declaration.json"'

execute 'Wait until the deadline to prove it' "storagext-cli system wait-for-height $SECOND_DEADLINE_START"
execute 'Submitting windowed post' 'storagext-cli --sr25519-key "//Charlie" storage-provider submit-windowed-post "@windowed-post.json"'

execute 'Wait until the deal end' "storagext-cli system wait-for-height $DEAL_END"
execute 'Settle deal payments' "storagext-cli --sr25519-key //Charlie market settle-deal-payments $DEAL_ID"
execute "Withdraw balance from Charlie's account" 'storagext-cli --sr25519-key "//Charlie" market withdraw-balance 37500000000'

echo 'Execution finished'
