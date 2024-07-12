# Polka Storage Node - Parachain

## Build the Docker Image

```bash
just build-parachain-docker
```

The command builds a Docker image `ghcr.io/eiger/polka-storage-node:0.1.0` from a Dockerfile located at `./docker/dockerfiles/parachain/Dockerfile`.

## Running the Parachain

Prerequisites:
- Kubernetes Cluster access - configured kubectl, e.g. [minikube](https://minikube.sigs.k8s.io/docs/start/)


Ideally, when the docker image `ghcr.io/eiger/polka-storage-node:0.1.0` is released publically, it'd work:

```bash
just kube-testnet
```
It launches zombienet from scratch based on `./zombienet/local-kube-testnet.toml` configuration.

### Workaround

However, given that ZombieNet [does not support private Docker registries](https://github.com/paritytech/zombienet/issues/1829) we need to do some trickery to test it out in Kubernetes.

It requires:
1. loading the image into the local minikube cluster
```bash
just load-to-minikube
```
2. building patched ZombieNet
    - requires NodeJS: preferably via [nvm](https://nodejs.org/en/download/package-manager)
    - pulling a branch of [ZombieNet](https://github.com/paritytech/zombienet/pull/1830) and building it locally
```bash
    git clone -b th7nder/fix/generating-containers git@github.com:th7nder/zombienet.git ~/patched-zombienet
    cd ~/patched-zombienet/javascript
    npm i && npm run build
    npm run package
```
3. running patched ZombieNet (inside of `polka-storage` workspace)
```bash
    ~/patched-zombienet/javascript/bins/zombienet-linux-x64 -p kubernetes spawn zombienet/local-kube-testnet.toml
```
