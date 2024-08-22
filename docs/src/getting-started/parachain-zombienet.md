# Parachain zombienet

This guide will help you to setup a local parachain network using zombienet. At the end we will have three nodes: Alice, Bob and Charlie. Alice and Bob will be running Polkadot relay chain nodes as validators, and Charlie will be running a relay chain and parachain node. Charlie will be our contact point to the parachain network.

## Prerequisites

- [minikube](https://minikube.sigs.k8s.io/docs/start/)
- kubectl (optional):
  - [https://minikube.sigs.k8s.io/docs/handbook/kubectl/](https://minikube.sigs.k8s.io/docs/handbook/kubectl/)
  - [https://kubernetes.io/docs/tasks/tools/#kubectl](https://kubernetes.io/docs/tasks/tools/#kubectl)

## Setting Up the Environment (minikube)

Start your Kubernetes cluster.

```
minikube start
```

## Running the Parachain

1. Copy the [local-kube-testnet.toml](../misc/local-kube-testnet.toml) file to your local machine.

2. Run the parachain, spawn the zombienet testnet in the Kubernetes cluster:

```
zombienet -p kubernetes spawn local-kube-testnet.toml
```

<details>
<summary>Click here to show the example output.</summary>

```
TODO
```

</details>

## Verifying the Setup

Check if all zombienet pods were started successfully:

`kubectl get pods --all-namespaces`

<details>
<summary>Click here to show the example output.</summary>

```
...
zombie-01b7920d650c18d3d78f75fd8b0978af   alice                              1/1     Running     0               77s
zombie-01b7920d650c18d3d78f75fd8b0978af   bob                                1/1     Running     0               62s
zombie-01b7920d650c18d3d78f75fd8b0978af   charlie                            1/1     Running     0               49s
zombie-01b7920d650c18d3d78f75fd8b0978af   fileserver                         1/1     Running     0               2m28s
zombie-01b7920d650c18d3d78f75fd8b0978af   temp                               0/1     Completed   0               2m25s
zombie-01b7920d650c18d3d78f75fd8b0978af   temp-1                             0/1     Completed   0               2m25s
zombie-01b7920d650c18d3d78f75fd8b0978af   temp-2                             0/1     Completed   0               2m15s
zombie-01b7920d650c18d3d78f75fd8b0978af   temp-3                             0/1     Completed   0               2m1s
zombie-01b7920d650c18d3d78f75fd8b0978af   temp-4                             0/1     Completed   0               114s
zombie-01b7920d650c18d3d78f75fd8b0978af   temp-5                             0/1     Completed   0               91s
zombie-01b7920d650c18d3d78f75fd8b0978af   temp-collator                      0/1     Completed   0               104s
```

</details>

## Accessing the Parachain

To interact with the parachain, you'll need to connect to Charlie's node on port `42069`. The port is configured in [local-kube-testnet.toml](../misc/local-kube-testnet.toml) under `rpc_port` for Charlie's node.
