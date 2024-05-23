alias b := build
alias t := testnet

lint:
    taplo lint && taplo fmt --check

build: lint
    cargo build --release

testnet: build
    zombienet -p native spawn zombienet/local-testnet.toml
