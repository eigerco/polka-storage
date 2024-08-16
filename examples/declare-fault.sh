#!/bin/bash -xe

# Export logging level and the storage provider key for simpler scripting
export RUST_LOG=trace
export SR25519_KEY="//Charlie"

# We start by creating two market accounts, to do that, we simply need to add money to the market balance account.
# We'll do that for Alice (our client) and for Charlie (our storage provider):

# Alice, using an explicit key
target/debug/storagext-cli --sr25519-key //Alice market add-balance 25100200300

# Charlie, with the implicit key, read from the SR25519_KEY environment variable
target/debug/storagext-cli market add-balance 25100200300

# We still don't have a registered storage provider, so let's register Charlie;
# once again, we're using the SR25519_KEY environment variable.
target/debug/storagext-cli storage-provider register charlie

# We then register the deal between Alice and Charlie.
#
# We're using Heredoc to keep everything in the current script,
# however, you can (and should) use a file path prepended with an @ symbol,
# the expected input is the same and you can see the file in examples/deals.json.
target/debug/storagext-cli --client-sr25519-key //Alice market publish-storage-deals << EOF
[
    {
        "piece_cid": "bafk2bzacecg3xxc4f2ql2hreiuy767u6r72ekdz54k7luieknboaakhft5rgk",
        "piece_size": 1,
        "client": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
        "provider": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
        "label": "dead",
        "start_block": 30,
        "end_block": 55,
        "storage_price_per_block": 1,
        "provider_collateral": 1,
        "state": "Published"
    }
]
EOF

# The provider now needs to pre-commit the received data,
# if in 100 blocks (the `expiration` field) this data isn't proven,
# the storage provider will receive a penalty (get his funds slashed).
target/debug/storagext-cli storage-provider pre-commit << EOF
{
    "sector_number": 1,
    "sealed_cid": "bafk2bzaceajreoxfdcpdvitpvxm7vkpvcimlob5ejebqgqidjkz4qoug4q6zu",
    "deal_ids": [0],
    "expiration": 100,
    "unsealed_cid": "bafk2bzaceajreoxfdcpdvitpvxm7vkpvcimlob5ejebqgqidjkz4qoug4q6zu",
    "seal_proof": "StackedDRG2KiBV1P1"
}
EOF

# Prove that we've properly stored the client's data.
target/debug/storagext-cli storage-provider prove-commit << EOF
{
    "sector_number": 1,
    "proof": "1230deadbeef"
}
EOF

# Let's now pretend that Charlie did an oopsie and the data the client trusted him has an issue,
# to avoid getting an harsh penalty, Charlie needs to assume his mistake by declaring a fault:
target/debug/storagext-cli storage-provider declare-faults << EOF
[
    {
        "deadline": 0,
        "partition": 0,
        "sectors": [
            1
        ]
    }
]
EOF
