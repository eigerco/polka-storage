alias b := build
alias r := release
alias t := testnet
alias f := fmt

# Generate the `metadata.scale` file, requires the node to be up and running at `127.0.0.1:42069`
generate-scale:
    subxt metadata -a --url http://127.0.0.1:42069 > cli/artifacts/metadata.scale

# Lint the project
lint:
    cargo clippy --locked --no-deps -- -D warnings
    taplo lint && taplo fmt --check

# Build the project in debug mode, linting it before
build: lint
    cargo build

# Build the project in release mode, linting it before
release: lint
    cargo build --release

# Build the testnet binaries in release mode
release-testnet:
    cargo build --release --features polka-storage-runtime/testnet --bin polka-storage-node

# Run the testnet without building
run-testnet:
    zombienet -p native spawn zombienet/local-testnet.toml

# Run the testing building it before
testnet: release-testnet run-testnet

test:
    cargo test --locked --workspace

fmt:
    taplo fmt
    cargo +nightly fmt

# Serve the MDBook
docs:
    mdbook serve -d docs/book docs/

# Build the polka storage node binary
build-polka-storage-node:
  cargo build --release --features polka-storage-runtime/testnet -p polka-storage-node

# Build the polka storage provider client
build-polka-storage-provider-client:
  cargo build --release -p polka-storage-provider-client

# Build the polka storage provider server
build-polka-storage-provider-server:
  cargo build --release -p polka-storage-provider-server

# Build the storagext cli binary
build-storagext-cli:
  cargo build --release --features storagext/insecure_url -p storagext-cli

# Build the mater cli binary
build-mater-cli:
  cargo build --release -p mater-cli

# Build all storage client related
build-client-binaries: build-mater-cli build-polka-storage-provider-client build-storagext-cli

# Build all storage provider binaries
build-provider-binaries: build-polka-storage-node build-polka-storage-provider-server

# Build all the binaries
build-binaries: build-client-binaries build-provider-binaries

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

# The tarpaulin calls for test coverage have the following options:
# --locked: To not update the Cargo.lock file.
# --skip-clean: Prevents tarpaulin from running `cargo clean` to reduce runtime.
# --fail-immediately: Makes tarpaulin stop when a test fails.
# --out: Specifies the output type, html for humans, lcov for Coverage Gutters.
# --output-dir: Specifies the output directory, these must be in sync with .vscode/settings.json and have extension Coverage Gutters to display it in VS Code.

pallet-storage-provider-coverage:
    mkdir -p coverage
    cargo tarpaulin -p pallet-storage-provider --locked --skip-clean --fail-immediately --out html lcov --output-dir coverage/pallet-storage-provider

market-coverage:
    mkdir -p coverage
    cargo tarpaulin -p pallet-market --locked --skip-clean --fail-immediately --out html lcov --output-dir coverage/pallet-market

mater-coverage:
    mkdir -p coverage
    cargo tarpaulin -p mater --locked --skip-clean --fail-immediately --out html lcov --output-dir coverage/mater

full-coverage: pallet-storage-provider-coverage market-coverage mater-coverage
