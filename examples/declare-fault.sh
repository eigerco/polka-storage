#!/bin/bash -xe

export RUST_LOG=trace
export SR25519_KEY="//Charlie"

target/debug/storagext-cli storage-provider register charlie

target/debug/storagext-cli --sr25519-key //Alice market add-balance 25100200300
target/debug/storagext-cli market add-balance 25100200300
target/debug/storagext-cli market publish-storage-deals --client-sr25519-key //Alice @examples/deals.json

target/debug/storagext-cli storage-provider pre-commit @examples/pre-commit-sector.json
target/debug/storagext-cli storage-provider prove-commit @examples/prove-commit-sector.json

target/debug/storagext-cli storage-provider declare-faults @examples/fault-declaration.json
