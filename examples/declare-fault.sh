#!/bin/bash -xe

cargo build -r -p storagext-cli

# Export logging level and the storage provider key for simpler scripting
# export RUST_LOG=trace
export SR25519_KEY="//Charlie"

# We start by creating two market accounts, to do that, we simply need to add money to the market balance account.
# We'll do that for Alice (our client) and for Charlie (our storage provider):

# Alice, using an explicit key
target/release/storagext-cli --sr25519-key "//Alice" market add-balance 25100200300

# Charlie, with the implicit key, read from the SR25519_KEY environment variable
target/release/storagext-cli market add-balance 25100200300

# We still don't have a registered storage provider, so let's register Charlie;
# once again, we're using the SR25519_KEY environment variable.
target/release/storagext-cli storage-provider register charlie

# We then register the deal between Alice and Charlie.
target/release/storagext-cli market publish-storage-deals --client-sr25519-key "//Alice" "@examples/deals.json"

# The provider now needs to pre-commit the received data,
# if in 100 blocks (the `expiration` field) this data isn't proven,
# the storage provider will receive a penalty (get his funds slashed).
target/release/storagext-cli storage-provider pre-commit "@examples/pre-commit-sector.json"

# Prove that we've properly stored the client's data.
target/release/storagext-cli storage-provider prove-commit "@examples/prove-commit-sector.json"

# Let's now pretend that Charlie did an oopsie and the data the client trusted him has an issue,
# to avoid getting an harsh penalty, Charlie needs to assume his mistake by declaring a fault:
target/release/storagext-cli storage-provider declare-faults "@examples/fault-declaration.json"

# In the meantime, Charlie undid his oopsie and can now say the sector is good for usage again:
target/release/storagext-cli storage-provider declare-faults-recovered "@examples/fault-declaration.json"

# To fully undo his oopsie, Charlie needs to submit a new proof for the previously faulty sector,
# this ensures that Charlie isn't lying about the sector recovery!
target/release/storagext-cli storage-provider submit-windowed-post "@examples/windowed-proof.json"
