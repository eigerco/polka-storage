export CARGO_WASM_RUNTIME_PATH := "target/release/wbuild/polka-storage-runtime/polka_storage_runtime.compact.compressed.wasm"

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
    cargo build --release --features polka-storage-runtime/testnet -p polka-storage-node --bin polka-storage-node

# Generate a private key for the P2P network and
# run the testnet without building
run-testnet:
    mkdir -p /tmp/zombienet
    openssl genpkey -algorithm ED25519 -out /tmp/zombienet/charlie-private.pem
    openssl pkey -in /tmp/zombienet/charlie-private.pem -pubout -out /tmp/zombienet/charlie-public.pem # Generate public key so script can get the Peer ID
    zombienet -p native spawn zombienet/local-testnet.toml

run-omni-testnet:
    cargo b -r -F testnet -p polka-storage-runtime
    chain-spec-builder create \
        -t local \
        -r $CARGO_WASM_RUNTIME_PATH \
        --relay-chain rococo-local \
        --para-id 1000 \
        named-preset local_testnet

    mkdir -p /tmp/zombienet
    zombienet -p native spawn zombienet/local-omni-testnet.toml

# Run a single collator
run-collator:
    mkdir -p /tmp/zombienet
    openssl genpkey -algorithm ED25519 -out /tmp/zombienet/david-private.pem
    openssl pkey -in /tmp/zombienet/david-private.pem -pubout -out /tmp/zombienet/david-public.pem # Generate public key so script can get the Peer ID
    zombienet -p native spawn zombienet/local-david-collator.toml

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
  cargo build --release --features polka-storage-runtime/testnet -p polka-storage-node --bin polka-storage-node

# Build the polka storage provider client
build-polka-storage-provider-client:
  if [ -n "${POLKA_STORAGE_CUDA+x}" ] && [ "${POLKA_STORAGE_CUDA}" = "true" ]; then \
    cargo build --release -p polka-storage-provider-client --no-default-features --features cuda; \
  else \
    cargo build --release -p polka-storage-provider-client; \
  fi

# Build the polka storage provider server
build-polka-storage-provider-server:
  if [ -n "${POLKA_STORAGE_CUDA+x}" ] && [ "${POLKA_STORAGE_CUDA}" = "true" ]; then \
    cargo build --release -p polka-storage-provider-server --no-default-features --features cuda; \
  else \
    cargo build --release -p polka-storage-provider-server; \
  fi

build-polka-storage-provider: build-polka-storage-provider-server build-polka-storage-provider-client

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
    mkdir -p /tmp/zombienet
    openssl genpkey -algorithm ED25519 -out /tmp/zombienet/charlie-private.pem
    openssl pkey -in /tmp/zombienet/charlie-private.pem -pubout -out /tmp/zombienet/charlie-public.pem # Generate public key so script can get the Peer ID
    zombienet -p kubernetes spawn zombienet/local-kube-testnet.toml

# The tarpaulin calls for test coverage have the following options:
# --locked: To not update the Cargo.lock file.
# --skip-clean: Prevents tarpaulin from running `cargo clean` to reduce runtime.
# --fail-immediately: Makes tarpaulin stop when a test fails.
# --out: Specifies the output type, html for humans, lcov for Coverage Gutters.
# --output-dir: Specifies the output directory, these must be in sync with .vscode/settings.json and have extension Coverage Gutters to display it in VS Code.

pallet-storage-provider-coverage:
    mkdir -p coverage
    cargo tarpaulin -p pallet-storage-provider -p pallet-storage-provider-benchmarks --locked --skip-clean --fail-immediately --out html lcov --output-dir coverage/pallet-storage-provider

market-coverage:
    mkdir -p coverage
    cargo tarpaulin -p pallet-market -p pallet-market-benchmarks --locked --skip-clean --fail-immediately --out html lcov --output-dir coverage/pallet-market

mater-coverage:
    mkdir -p coverage
    cargo tarpaulin -p mater --locked --skip-clean --fail-immediately --out html lcov --output-dir coverage/mater

full-coverage: pallet-storage-provider-coverage market-coverage mater-coverage

# Generate both PoRep and PoSt parameters given the size
generate-proof-params sector-size:
    cargo r -r -p polka-storage-provider-client -- proofs porep-params --seal-proof "{{sector-size}}"
    cargo r -r -p polka-storage-provider-client -- proofs post-params --post-type "{{sector-size}}"

# sector-size = 8MiB, 1GiB available on the blob store
download-params sector-size:
    mkdir -p target/params
    wget -O target/params/{{sector-size}}.porep.params https://polkastore.blob.core.windows.net/params/{{sector-size}}.porep.params
    wget -O target/params/{{sector-size}}.post.params https://polkastore.blob.core.windows.net/params/{{sector-size}}.post.params

# Run the benchmark tests
bench-test pallet:
    cargo test --profile ci --locked -p "pallet-{{pallet}}-benchmarks" --features runtime-benchmarks -- benchmark --nocapture

# Run benchmarks
bench-node pallet steps="5" repeat="1":
    cargo b -r -p polka-storage-runtime -F testnet -F runtime-benchmarks

    frame-omni-bencher v1 \
        benchmark pallet \
        --runtime target/release/wbuild/polka-storage-runtime/polka_storage_runtime.compact.compressed.wasm \
        --pallet "pallet_{{pallet}}" \
        --extrinsic "*" \
        --steps "{{steps}}" \
        --repeat "{{repeat}}"


# Generate the benchmark weights
generate-weights pallet steps="5" repeat="1":
    cargo b -r -p polka-storage-runtime -F testnet -F runtime-benchmarks

    frame-omni-bencher v1 \
        benchmark pallet \
        --runtime target/release/wbuild/polka-storage-runtime/polka_storage_runtime.compact.compressed.wasm \
        --pallet "pallet_{{pallet}}" \
        --extrinsic "*" \
        --steps "{{steps}}" \
        --repeat "{{repeat}}" \
        --template node/benchmark_template.hbs \
        --output "pallets/{{pallet}}/src/weights.rs"

build-runtime-deterministic:
    # Init and start the podman VM. The 8GB of RAM is not immediately allocated, but the higher-than-default cap
    # is necessary to build the runtime. The volume mount is necessary *if* the current working directory is not
    # on the list of default mounts. The command will "fail" if the machine already exists and/or is running,
    # so all errors are ignored for simplicity. The command output should be enough to debug any issues.
    podman machine init --memory=8192 -v "$(pwd):$(pwd)" || true
    podman machine start || true
    # Build the runtime. The --root is needed because of https://github.com/paritytech/srtool/issues/46
    srtool build \
        --app \
        --package polka-storage-runtime \
        --runtime-dir runtime \
        --verbose \
        --build-opts='"--features testnet"' \
        --root \
        | tail -n1 | jq .  > target/runtime.json

build-plain-chain-spec chain para-id:
    chain-spec-builder --chain-spec-path "chainspecs/{{ chain }}.plain.json" create \
        --relay-chain {{ chain }} \
        --para-id {{ para-id }} \
        --chain-name "Polka Storage" \
        --chain-id "polka-storage" \
        -t live \
        --verify \
        --runtime runtime/target/srtool/release/wbuild/polka-storage-runtime/polka_storage_runtime.compact.compressed.wasm \
        patch chainspecs/{{ chain }}.patch.json

build-raw-chain-spec chain para-id: (build-plain-chain-spec chain para-id)
    chain-spec-builder --chain-spec-path "chainspecs/{{ chain }}.raw.json" convert-to-raw \
    "chainspecs/{{ chain }}.plain.json"

build-genesis chain para-id: (build-raw-chain-spec chain para-id)
    target/release/polka-storage-node export-genesis-wasm --chain "chainspecs/{{ chain }}.raw.json" "target/{{ chain }}.para-wasm"
    target/release/polka-storage-node export-genesis-state --chain "chainspecs/{{ chain }}.raw.json" "target/{{ chain }}.para-state"

run-polka-storage-node chain:
    target/release/polka-storage-node --collator \
        --chain chainspecs/{{ chain }}.raw.json \
        --base-path data \
        --rpc-port 42069 \
        --force-authoring \
        --node-key-file ./data/chains/polka-storage/network/secret_ed25519 \
        --p2p-key $(cat ./data/chains/polka-storage/network/secret_ed25519) \
        --pool-type fork-aware \
        -- \
        --discover-local \
        --sync warp \
        --chain {{ chain }}

run-bootstrap:
    # openssl genpkey -algorithm ED25519 -out /tmp/zombienet/charlie-private.pem
    # openssl pkey -in /tmp/zombienet/charlie-private.pem -pubout -out /tmp/zombienet/charlie-public.pem # Generate public key so script can get the Peer ID
    RUST_LOG=info,polka_storage_bootstrap=trace cargo r -r -p polka-storage-bootstrap -- \
        run \
        --listen-addresses "/ip4/0.0.0.0/tcp/5678,/ip4/0.0.0.0/tcp/5679/ws"
    # --bootstrap-addresses "/ip4/127.0.0.1/tcp/51788/ws/p2p/12D3KooWQCkBm1BYtkHpocxCwMgR8yjitEeHGx8spzcDLGt2gkBm" \
    # --keypair "@/tmp/zombienet/charlie-private.pem"