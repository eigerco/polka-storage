alias b := build
alias r := release
alias t := testnet

lint:
    taplo lint && taplo fmt --check

build: lint
    cargo build

release: lint
    cargo build --release

testnet: release
    zombienet -p native spawn zombienet/local-testnet.toml
