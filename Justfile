alias b := build
alias r := release
alias t := testnet

generate-scale:
    subxt metadata -a --url http://127.0.0.1:42069 > cli/artifacts/metadata.scale

lint:
    cargo clippy --locked --no-deps -- -D warnings
    taplo lint && taplo fmt --check

build: lint
    cargo build

release: lint
    cargo build --release

release-testnet: lint
    cargo build --release --features polka-storage-runtime/testnet

testnet: release-testnet
    zombienet -p native spawn zombienet/local-testnet.toml

test:
    cargo test --locked --workspace

build-parachain-docker:
    docker build \
        --build-arg VCS_REF="$(git rev-parse HEAD)" \
        --build-arg BUILD_DATE="$(date -u +'%Y-%m-%dT%H:%M:%SZ')" \
        -t polkadotstorage.azurecr.io/parachain-node:0.1.0 \
        --file ./docker/dockerfiles/parachain/Dockerfile \
        .
build-storage-provider-docker:
    docker build \
        --build-arg VCS_REF="$(git rev-parse HEAD)" \
        --build-arg BUILD_DATE="$(date -u +'%Y-%m-%dT%H:%M:%SZ')" \
        -t polkadotstorage.azurecr.io/polka-storage-provider:0.1.0 \
        --file ./docker/dockerfiles/storage-provider/Dockerfile \
        .

load-to-minikube:
    # https://github.com/paritytech/zombienet/pull/1830
    # unless this is merged and we pull it in, launching it in local zombienet (without publicly publishing the docker image is impossible)
    minikube image load polkadotstorage.azurecr.io/parachain-node:0.1.0

kube-testnet:
    zombienet -p kubernetes spawn zombienet/local-kube-testnet.toml
pallet-storage-provider-coverage:
    mkdir -p coverage
    cargo llvm-cov -p pallet-storage-provider --ignore-filename-regex "(mock|test)"
    cargo llvm-cov -p pallet-storage-provider report --ignore-filename-regex "(mock|test)" --html --output-dir coverage/pallet-storage-provider
    cargo llvm-cov -p pallet-storage-provider report --ignore-filename-regex "(mock|test)" --lcov --output-path coverage/pallet-storage-provider.lcov.info

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
