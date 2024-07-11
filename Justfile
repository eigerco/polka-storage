alias b := build
alias r := release
alias t := testnet

lint:
    cargo clippy --locked --no-deps -- -D warnings
    taplo lint && taplo fmt --check

build: lint
    cargo build

release: lint
    cargo build --release

testnet: release
    zombienet -p native spawn zombienet/local-testnet.toml

# Must be in sync with .vscode/settings.json and have extension Coverage Gutters to display it in VS Code.
market-coverage:
    mkdir -p coverage
    cargo llvm-cov -p pallet-market --ignore-filename-regex "(mock|test)"
    cargo llvm-cov -p pallet-market report --ignore-filename-regex "(mock|test)" --html --output-dir coverage/pallet-market
    cargo llvm-cov -p pallet-market report --ignore-filename-regex "(mock|test)" --lcov --output-path coverage/pallet-market.lcov.info

mater-coverage:
    cargo llvm-cov -p mater --ignore-filename-regex "(mock|test)"
    cargo llvm-cov -p mater report --ignore-filename-regex "(mock|test)" --html --output-dir coverage/mater
    cargo llvm-cov -p mater report --ignore-filename-regex "(mock|test)" --lcov --output-path coverage/mater.lcov.info