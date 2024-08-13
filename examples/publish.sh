#!/bin/bash 
target/debug/storagext-cli --sr25519-key //Charlie storage-provider register charlie

target/debug/storagext-cli --sr25519-key //Alice market add-balance 25100200300
target/debug/storagext-cli --sr25519-key //Charlie market add-balance 25100200300
RUST_LOG=trace target/debug/storagext-cli --sr25519-key //Charlie market publish-storage-deals --client-sr25519-key //Alice @examples/deals.json

target/debug/storagext-cli --sr25519-key //Charlie storage-provider pre-commit @examples/pre-commit-sector.json
target/debug/storagext-cli --sr25519-key //Charlie storage-provider prove-commit @examples/prove-commit-sector.json