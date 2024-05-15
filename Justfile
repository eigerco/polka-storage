alias b := build
alias r := release
alias t := testnet

build:
    cargo build

release:
    cargo build --release

testnet: release
    zombienet -p native spawn scripts/local-testnet.toml