#!/bin/bash -xe

# NOTE: we need to remove the target/debug

# Export logging level and the storage provider key for simpler scripting
export RUST_LOG=trace
export SR25519_KEY="//Charlie"

# We start by creating two market accounts, to do that, we simply need to add money to the market balance account.
# We'll do that for Alice (our client) and for Charlie (our storage provider):

# Alice, using an explicit key
target/debug/storagext-cli --sr25519-key "//Alice" market add-balance 25100200300

# Charlie, with the implicit key, read from the SR25519_KEY environment variable
target/debug/storagext-cli market add-balance 25100200300

# We still don't have a registered storage provider, so let's register Charlie;
# once again, we're using the SR25519_KEY environment variable.
target/debug/storagext-cli storage-provider register charlie

# We then register the deal between Alice and Charlie.
target/debug/storagext-cli market publish-storage-deals --client-sr25519-key "//Alice" "@examples/deals.json"

# The provider now needs to pre-commit the received data,
# if in 100 blocks (the `expiration` field) this data isn't proven,
# the storage provider will receive a penalty (get his funds slashed).
target/debug/storagext-cli storage-provider pre-commit "@examples/pre-commit-sector.json"

# Prove that we've properly stored the client's data.
target/debug/storagext-cli storage-provider prove-commit "@examples/prove-commit-sector.json"

# Let's now pretend that Charlie did an oopsie and the data the client trusted him has an issue,
# to avoid getting an harsh penalty, Charlie needs to assume his mistake by declaring a fault:
target/debug/storagext-cli storage-provider declare-faults "@examples/fault-declaration.json"

# In the meantime, Charlie undid his oopsie and can now say the sector is good for usage again:
target/debug/storagext-cli storage-provider declare-faults-recovered "@examples/fault-declaration.json"
