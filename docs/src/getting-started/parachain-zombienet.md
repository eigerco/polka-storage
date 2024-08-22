# Parachain zombienet

This guide will help you to setup a local parachain network using zombienet. At the end we will have three nodes: Alice, Bob and Charlie. Alice and Bob will be running Polkadot relay chain nodes as validators, and Charlie will be running a relay chain and parachain node. Charlie will be our contact point to the parachain network.

## Prerequisites

- [minikube](https://minikube.sigs.k8s.io/docs/start/)
- Kubernetes Cluster access - configured [kubectl](https://kubernetes.io/docs/tasks/tools/#kubectl) (optional, but recommended for local testing)

## Setting Up the Environment (minikube)

Start your Kubernetes cluster.

```
minikube start
```

## Running the Parachain

To run the parachain, spawn the zombienet testnet in the Kubernetes cluster:

```
zombienet -p kubernetes spawn zombienet/local-kube-testnet.toml
```

## Verifying the Setup

To check the status of your Kubernetes cluster:

`kubectl get pods --all-namespaces`

This command will show all pods from all namespaces along with their status.

## Accessing the Parachain

To interact with the parachain, you'll need to connect to Charlie's node on port 42069. The port is configured in `zombienet/local-kube-testnet.toml`.
