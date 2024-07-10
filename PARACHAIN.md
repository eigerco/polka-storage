# Polka Storage Node - Parachain

## Build the Docker Image

```bash
docker build \
    --build-arg VCS_REF=$(git rev-parse HEAD) \
    --build-arg BUILD_DATE=$(date -u +'%Y-%m-%dT%H:%M:%SZ') \
    -t eiger/polka-storage-node \
    --file ./docker/dockerfiles/parachain/Dockerfile \
    .
```

The command builds a Docker image `eiger/polka-storage-node` from a Dockerfile located at `./docker/dockerfiles/parachain/Dockerfile`.

## Running the Parachain

`just podman-testnet`