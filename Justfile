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

build-parachain-docker:
    docker build \
        --build-arg VCS_REF=$(git rev-parse HEAD) \
        --build-arg BUILD_DATE=$(date -u +'%Y-%m-%dT%H:%M:%SZ') \
        -t ghcr.io/eigerco/polka-storage-node:0.1.0 \
        --file ./docker/dockerfiles/parachain/Dockerfile \
        .
        
load-to-minikube:
    # https://github.com/paritytech/zombienet/pull/1830
    # untill this is merged and we pull it in, launching it in local zombienet (without publishing the docker image is impossible)
    minikube image load ghcr.io/eigerco/polka-storage-node:0.1.0 

kube-testnet:
    zombienet -p kubernetes spawn zombienet/local-kube-testnet.toml

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