# Polka Storage Node - Parachain

Supported Kubernetes Platforms:
- Linux x86_64

Others were not tested and `polkadot` does not have an image for Linux ARM.

## Build the Docker Image

```bash
just build-parachain-docker
```

The command builds a Docker image `polkadotstorage.azurecr.io/parachain-node:0.1.0` from a Dockerfile located at `./docker/dockerfiles/parachain/Dockerfile`.

## Authenticating and Authorizing to Azure Container Registry

```bash
az login --use-device-code --tenant dareneiger.onmicrosoft.com
az acr login --name polkadotstorage
```

> [!NOTE]
> Azure Container Registry token expires after 3 hours.

## Pulling the image from registry

Use this option if you don't want to build locally.

```bash
docker pull polkadotstorage.azurecr.io/parachain-node:0.1.0
```

## Running the Parachain

Prerequisites:
- Kubernetes Cluster access - configured [kubectl](https://kubernetes.io/docs/tasks/tools/#kubectl), e.g. [minikube](https://minikube.sigs.k8s.io/docs/start/),
- If using `minikube`, a started minikube cluster (`minikube start`).

The configuration is stored in `./zombienet/local-kube-testnet.toml`.
ZombieNet [does not support private Docker registries](https://github.com/paritytech/zombienet/issues/1829) we need to do some trickery to test it out in Kubernetes.

It requires:
1. Loading the image into the local minikube cluster
```bash
just load-to-minikube
```
2. Building patched ZombieNet
    - requires NodeJS (LTS v20): preferably via [nvm](https://nodejs.org/en/download/package-manager)
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

## Zombienet Structure

The previous setup leaves us with 3 nodes:
* Alice
* Bob
* Charlie

The first two (Alice and Bob) will be running Polkadot relay chain nodes, as such,
you won't have access to the parachain extrinsics when calling them.

Charlie however, is running a parachain node, and as such, he will be your contact point to the parachain.

> Check you Kubernetes cluster status by using `kubectl get pods --all-namespaces`.
> It should show all pods from all namespaces along with their status.