# Polka Storage Node - Parachain

Supported Kubernetes Platforms:
- Linux x86_64

Others were not tested and `polkadot` does not have an image for Linux ARM.

## Build the Docker Image

```bash
just build-parachain-docker
```

The command builds a Docker image `polkadotstorage.azurecr.io/parachain-node:0.1.0` from a Dockerfile located at `./docker/dockerfiles/parachain/Dockerfile`.

## Running the Parachain

Prerequisites:
- Kubernetes Cluster access - configured [kubectl](https://kubernetes.io/docs/tasks/tools/#kubectl), e.g. [minikube](https://minikube.sigs.k8s.io/docs/start/)

The configuration is stored in `./zombienet/local-kube-testnet.toml`.
ZombieNet [does not support private Docker registries](https://github.com/paritytech/zombienet/issues/1829) we need to do some trickery to test it out in Kubernetes.

It requires:
1. Loading the image into the local minikube cluster
```bash
just load-to-minikube
```
2. Building patched ZombieNet
    - requires NodeJS: preferably via [nvm](https://nodejs.org/en/download/package-manager)
    - pulling a branch of [ZombieNet](https://github.com/paritytech/zombienet/pull/1830) and building it locally
    ```bash
        git clone -b th7nder/fix/generating-containers git@github.com:th7nder/zombienet.git patched-zombienet
        cd patched-zombienet/javascript
        npm i && npm run build
        npm run package
    ```
    - NOTE: warnings like `> Warning Failed to make bytecode node18-arm64 for file` are normal and don't break the build.
3. Finally running patched ZombieNet (inside of `polka-storage` workspace)
```bash
patched-zombienet/javascript/bins/zombienet-linux-x64 -p kubernetes spawn zombienet/local-kube-testnet.toml
```
