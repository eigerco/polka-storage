# Docker Setup

This guide will outline how to setup your environment using Docker to get started with the Polka Storage parachain.

## Pre-requisites

Install Docker on your system by following the [Docker install instructions](https://docs.docker.com/engine/install/).

> Podman _may_ work, however, we do not provide any guarantees.

## Dockerfile setup

All docker builds are composed of 4 stages.

1. Set up [`cargo chef`](https://github.com/LukeMathWalker/cargo-chef), this caches the Rust dependencies for faster builds.
2. Planning — `cargo chef` analyzes the current project to determine the minimum subset of file required to build it an cache the dependencies.
3. Build — `cargo chef` checks the project skeleton identified in the planner stage and builds it to cache dependencies.
4. Runtime — sets up the runtime with Debian and imports the binary build in the previous stage.

## Building & Running

Clone the repository and go into the directory:

```shell
git clone git@github.com:eigerco/polka-storage.git
cd polka-storage
```

You can find Dockerfiles for each binary under the `docker/` folder.
To build the images manually you can use the following command:

```bash
docker build \
        --build-arg VCS_REF="$(git rev-parse HEAD)" \
        --build-arg BUILD_DATE="$(date -u +'%Y-%m-%dT%H:%M:%SZ')" \
        -t <DOCKERFILE-NAME>:"$(cargo metadata --format-version=1 --no-deps | jq -r '.packages[0].version')" \
        --file ./docker/dockerfiles/<DOCKERFILE-NAME>.Dockerfile \
        .
```

Where you can replace `<DOCKERFILE-NAME>` by one of the following:

- `polka-storage-node`
- `polka-storage-provider-server`
- `polka-storage-provider-client`
- `mater-cli`
- `storagext-cli`

To run the images manually, you apply the same pattern to the following command:

```bash
docker run -it polkadotstorage.azurecr.io/<DOCKERFILE-NAME>:"$(cargo metadata --format-version=1 --no-deps | jq -r '.packages[0].version')"
```

### Just recipes

To simplify the building process, we've written some [Just](https://github.com/casey/just) recipes.

| Command                                      | Description                                                                                                               |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `build-mater-docker`                         | Builds the `mater` CLI image which is used by storage clients to convert files to CARv2 format and extract CARv2 content. |
| `build-polka-storage-node-docker`            | Builds the Polka Storage parachain node image.                                                                            |
| `build-polka-storage-provider-server-docker` | Builds the Storage Provider server image.                                                                                 |
| `build-polka-storage-provider-client-docker` | Builds the Storage Provider client image.                                                                                 |
| `build-storagext-docker`                     | Builds the `storagext` CLI image used to execute extrinsics.                                                              |
| `build-docker-all`                           | Builds all the images above, this might take a while to complete.                                                         |
| `run-mater-docker`                           | Runs the image, opening a shell with access to the `mater-cli` binary.                                                    |
| `run-polka-storage-node-docker`              | Runs the `polka-storage-node` inside the built Docker image.                                                              |
| `run-polka-storage-provider-server-docker`   | Runs the image, opening a shell with access to the `polka-storage-provider-server` binary.                                |
| `run-polka-storage-provider-client-docker`   | Runs the image, opening a shell with access to the `polka-storage-provider-client` binary.                                |
| `run-storagext-docker`                       | Runs the image, opening a shell with access to the `storagext-cli` binary.                                                |
