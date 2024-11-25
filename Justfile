alias b := build
alias r := release
alias t := testnet
alias f := fmt

# Generate the `metadata.scale` file, requires the node to be up and running at `127.0.0.1:42069`
generate-scale:
    subxt metadata -a --url http://127.0.0.1:42069 > storagext/lib/artifacts/metadata.scale

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

# Build the storagext CLI binary
build-storagext-cli:
  cargo build --release -p storagext-cli

# Build the mater CLI binary
build-mater-cli:
  cargo build --release -p mater-cli

# Build all the binaries
build-binaries-all: build-polka-storage-node build-polka-storage-provider-client build-polka-storage-provider-server build-storagext-cli build-mater-cli


# NOTE: Docker builds have no ghcr prefix because these are built locally.
# The Docker images built in the CI will point to the ghcr.
# Done to differenciate between local images and pulled images.

# Build the mater CLI binary
build-mater-docker:
  docker build \
        --build-arg VCS_REF="$(git rev-parse HEAD)" \
        --build-arg BUILD_DATE="$(date -u +'%Y-%m-%dT%H:%M:%SZ')" \
        -t mater-cli:"$(cargo metadata --format-version=1 --no-deps | jq -r '.packages[] | select(.name == "mater-cli") | .version')" \
        --file ./docker/dockerfiles/mater-cli.Dockerfile \
        .

# Build the polka storage node docker image
build-polka-storage-node-docker:
    docker build \
        --build-arg VCS_REF="$(git rev-parse HEAD)" \
        --build-arg BUILD_DATE="$(date -u +'%Y-%m-%dT%H:%M:%SZ')" \
        -t polka-storage-node:"$(cargo metadata --format-version=1 --no-deps | jq -r '.packages[] | select(.name == "polka-storage-node")| .version')" \
        --file ./docker/dockerfiles/polka-storage-node.Dockerfile \
        .

# Build the polka storage provider client docker image
build-polka-storage-provider-client-docker:
  docker build \
        --build-arg VCS_REF="$(git rev-parse HEAD)" \
        --build-arg BUILD_DATE="$(date -u +'%Y-%m-%dT%H:%M:%SZ')" \
        -t polka-storage-provider-client:"$(cargo metadata --format-version=1 --no-deps | jq -r '.packages[] | select(.name == "polka-storage-provider-client")| .version')" \
        --file ./docker/dockerfiles/polka-storage-provider-client.Dockerfile \
        .

# Build the polka storage provider server docker image
build-polka-storage-provider-server-docker:
  docker build \
        --build-arg VCS_REF="$(git rev-parse HEAD)" \
        --build-arg BUILD_DATE="$(date -u +'%Y-%m-%dT%H:%M:%SZ')" \
        -t polka-storage-provider-server:"$(cargo metadata --format-version=1 --no-deps | jq -r '.packages[] | select(.name == "polka-storage-provider-server")| .version')" \
        --file ./docker/dockerfiles/polka-storage-provider-server.Dockerfile \
        .

# Build the storagext CLI docker image
build-storagext-docker:
  docker build \
        --build-arg VCS_REF="$(git rev-parse HEAD)" \
        --build-arg BUILD_DATE="$(date -u +'%Y-%m-%dT%H:%M:%SZ')" \
        -t storagext-cli:"$(cargo metadata --format-version=1 --no-deps | jq -r '.packages[] | select(.name == "storagext-cli")| .version')" \
        --file ./docker/dockerfiles/storagext-cli.Dockerfile \
        .

# Builds all docker image.
# This operation will take a while
build-docker-all: build-polka-storage-node-docker build-polka-storage-provider-client-docker build-polka-storage-provider-server-docker build-storagext-docker build-mater-docker

# Run the mater CLI docker image
# This only works if the image is already built
run-mater-docker:
    docker run -it mater-cli:"$(cargo metadata --format-version=1 --no-deps | jq -r '.packages[] | select(.name == "mater-cli")| .version')"

# Run the parachain node docker image
# This only works if the image is already built
run-polka-storage-node-docker:
    docker run -it parachain-node:"$(cargo metadata --format-version=1 --no-deps | jq -r '.packages[] | select(.name == "polka-storage-node")| .version')"

# Run the storage provider client docker image
# This only works if the image is already built
run-polka-storage-client-docker:
    docker run -it sp-client:"$(cargo metadata --format-version=1 --no-deps | jq -r '.packages[] | select(.name == "polka-storage-provider-client")| .version')"

# Run the storage provider server docker image
# This only works if the image is already built
run-polka-storage-server-docker:
    docker run -it sp-server:"$(cargo metadata --format-version=1 --no-deps | jq -r '.packages[] | select(.name == "polka-storage-provider-server")| .version')"

# Run the storagext CLI docker image
# This only works if the image is already built
run-storagext-docker:
    docker run -it storagext-cli:"$(cargo metadata --format-version=1 --no-deps | jq -r '.packages[] | select(.name == "storagext-cli")| .version')"

load-to-minikube:
    # https://github.com/paritytech/zombienet/pull/1830
    # unless this is merged and we pull it in, launching it in local zombienet (without publicly publishing the docker image is impossible)
    minikube image load ghcr.io/polka-storage-node:"$(cargo metadata --format-version=1 --no-deps | jq -r '.packages[] | select(.name == "polka-storage-node") | .version')"

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
