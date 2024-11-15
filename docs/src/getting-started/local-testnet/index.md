# Local Testnet - Polka Storage Parachain

This guide helps to set up a local parachain network using zombienet.
At the end, we will have three nodes: Alice, Bob and Charlie.
Alice and Bob will be running Polkadot relay chain nodes as validators,
and Charlie will run a relay chain and parachain node.
Charlie will be our contact point to the parachain network.

## Native Binaries

The binaries for the latest releases are available to download and can be ran without any additional dependencies.
We support `Linux x86_64` and `MacOS ARM x64`. The commands below will download:

- [Relay Chain](https://github.com/paritytech/polkadot-sdk/releases) binaries (`polkadot`, `polkadot-prepare-worker`, `polkadot-execute-worker`),
- Polka Storage Parachain binary (`polka-storage-node`),
- [Polka Storage Provider](../../storage-provider-cli/server.md) internal node (`polka-storage-provider-server`),
- [Polka Storage Provider Client](../../storage-provider-cli/client/index.md) internal node's RPC client and proving tools (`polka-storage-provider-client`),
- [Storagext CLI](../../storagext-cli/index.md) to interact with a chain by sending extrinsics (`storagext-cli`),
- [Mater CLI](../../mater-cli/index.md) for CARv2 file archive operations (`mater-cli`),
- [zombienet](https://paritytech.github.io/zombienet/install.html) to spawn local testnets and orchestrate them (`zombienet`),
- Polka Storage Parachain out-of-the-box zombienet's configuration (`polka-storage-testnet.toml`).

### Linux x86_64

1. Download the binaries:

```bash
wget https://github.com/paritytech/polkadot-sdk/releases/download/polkadot-stable2407-1/polkadot
wget https://github.com/paritytech/polkadot-sdk/releases/download/polkadot-stable2407-1/polkadot-prepare-worker
wget https://github.com/paritytech/polkadot-sdk/releases/download/polkadot-stable2407-1/polkadot-execute-worker
wget https://github.com/eigerco/polka-storage/releases/download/polka-storage-node-v0.0.0/polka-storage-node-linux-x86 -O polka-storage-node
wget https://github.com/eigerco/polka-storage/releases/download/polka-storage-provider-client-v0.1.0/polka-storage-provider-client-linux-x86 -O polka-storage-provider-client
wget https://github.com/eigerco/polka-storage/releases/download/polka-storage-provider-server-v0.1.0/polka-storage-provider-server-linux-x86 -O polka-storage-provider-server
wget https://github.com/eigerco/polka-storage/releases/download/storagext-cli-v0.1.0/storagext-cli-linux-x86 -O storagext-cli
wget https://github.com/eigerco/polka-storage/releases/download/mater-cli-v0.1.0/mater-cli-linux-x86 -O mater-cli
wget https://github.com/paritytech/zombienet/releases/download/v1.3.106/zombienet-linux-x64 -O zombienet
```

2. Setup permissions:

```bash
chmod +x zombienet polka-storage-node polka-storage-provider-client polka-storage-provider-server storagext-cli mater-cli polkadot polkadot-prepare-worker polkadot-execute-worker
```

3. Run `zombienet`:

```bash
export PATH=$(pwd):$PATH

wget https://s3.eu-central-1.amazonaws.com/polka-storage/polka-storage-testnet.toml
zombienet -p native spawn polka-storage-testnet.toml
```

### MacOS ARM

1. Download the binaries:

```bash
wget https://github.com/eigerco/polka-storage/releases/download/polka-storage-node-v0.0.0/polkadot-2407-1-macos-arm64 -O polkadot
wget https://github.com/eigerco/polka-storage/releases/download/polka-storage-node-v0.0.0/polkadot-2407-1-prepare-worker-macos-arm64 -O polkadot-prepare-worker
wget https://github.com/eigerco/polka-storage/releases/download/polka-storage-node-v0.0.0/polkadot-2407-1-execute-worker-macos-arm64 -O polkadot-execute-worker
wget https://github.com/eigerco/polka-storage/releases/download/polka-storage-node-v0.0.0/polka-storage-node-macos-arm64 -O polka-storage-node
wget https://github.com/eigerco/polka-storage/releases/download/polka-storage-provider-server-v0.1.0/polka-storage-provider-server-macos-arm64 -O polka-storage-provider-server
wget https://github.com/eigerco/polka-storage/releases/download/polka-storage-provider-client-v0.1.0/polka-storage-provider-client-macos-arm64 -O polka-storage-provider-client
wget https://github.com/eigerco/polka-storage/releases/download/storagext-cli-v0.1.0/storagext-cli-macos-arm64 -O storagext-cli
wget https://github.com/eigerco/polka-storage/releases/download/mater-cli-v0.1.0/mater-cli-macos-arm64 -O mater-cli
wget https://github.com/paritytech/zombienet/releases/download/v1.3.106/zombienet-macos-arm64 -O zombienet
```

2. Setup permissions & de-quarantine:

```bash
chmod +x zombienet polka-storage-node polka-storage-provider-server polka-storage-provider-client storagext-cli mater-cli polkadot polkadot-prepare-worker polkadot-execute-worker
xattr -d com.apple.quarantine zombienet polka-storage-node polka-storage-provider-server polka-storage-provider-client storagext-cli mater-cli polkadot polkadot-prepare-worker polkadot-execute-worker
```

<div class="warning">
If, when running the <code>xattr</code> command, it outputs <code>No such attr: com.apple.quarantine</code>, there's nothing to worry about. It means the downloaded binaries were not quarantined.
</div>

3. Run `zombienet`:

```bash
export PATH=$(pwd):$PATH

wget https://s3.eu-central-1.amazonaws.com/polka-storage/polka-storage-testnet.toml
zombienet -p native spawn polka-storage-testnet.toml
```

The parachain is also accessible using the Polkadot.js Apps interface by clicking on this link:
<https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:42069#/explorer>

<p>
  <a href="https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:42069#/explorer">
    <img src="../images/polkadot_substrate_portal.png" alt="Polkadot/Subtrate Portal">
  </a>
</p>

Or interact with the chain via [`storagext-cli`](../../storagext-cli/index.md), for example:

```bash
storagext-cli --sr25519-key "//Alice" storage-provider register Alice
```

## Kubernetes

<div class="warning">
Docker Images were only published on x86_64 platforms! They won't work on Kubernetes on MacOS.
</div>

### Prerequisites

- [zombienet v1.3.106](https://github.com/paritytech/zombienet/releases/tag/v1.3.106) - cli tool to easily spawn ephemeral Polkadot/Substrate networks and perform tests against them.
- [minikube](https://minikube.sigs.k8s.io/docs/start/) — to run the parachain nodes
- a configured [kubectl](https://kubernetes.io/docs/tasks/tools/#kubectl) — is used to set up the required kubernetes resources by `zombienet`
  - [https://minikube.sigs.k8s.io/docs/handbook/kubectl/](https://minikube.sigs.k8s.io/docs/handbook/kubectl/)
  - [https://kubernetes.io/docs/tasks/tools/#kubectl](https://kubernetes.io/docs/tasks/tools/#kubectl)

### Start up the Kubernetes cluster

Using `minikube`, start the cluster with the following command:

```
minikube start
```

More information about `minikube` is available on its [Getting Started](https://minikube.sigs.k8s.io/docs/handbook/controls/) page.

### Running the Parachain

1. Create a `local-kube-testnet.toml` file with the following content:

```toml
[settings]
image_pull_policy = "IfNotPresent"

[relaychain]
chain = "rococo-local"
default_args = ["--detailed-log-output", "-lparachain=debug,xcm=trace,runtime=trace"]
default_image = "docker.io/parity/polkadot:stable2407-1"

[[relaychain.nodes]]
name = "alice"
validator = true

[[relaychain.nodes]]
name = "bob"
validator = true

[[parachains]]
cumulus_based = true

# We need to use a Parachain of an existing System Chain (https://github.com/paritytech/polkadot-sdk/blob/master/polkadot/runtime/rococo/src/xcm_config.rs).
# The reason: being able to get native DOTs from Relay Chain to Parachain via XCM Teleport.
# We'll have a proper Parachain ID in the *future*, but for now, let's stick to 1000 (which is AssetHub and trusted).
id = 1000

# Run Charlie as parachain collator
[[parachains.collators]]
args = ["--detailed-log-output", "-lparachain=debug,xcm=trace,runtime=trace"]
command = "polka-storage-node"
image = "ghcr.io/eigerco/polka-storage-node:0.0.0"
name = "charlie"
rpc_port = 42069
validator = true
```

> For details on this configuration, refer to the [Zombienet Configuration](../../zombienet-config.md) chapter.

2. Run the Parachain, and spawn the zombienet testnet in the Kubernetes cluster:

```
zombienet -p kubernetes spawn local-kube-testnet.toml
```

<details>
<summary>Click here to show the example output.</summary>

```
│ /ip4/10.1.0.16/tcp/30333/ws/p2p/12D3KooWPKzmmE2uYgF3z13xjpbFTp63g9dZFag8pG6MgnpSLF4S                                   │
└────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘

         Warn: Tracing collator service doesn't exist
┌───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                                       Network launched 🚀🚀                                                       │
├──────────────────────────────┬────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Namespace                    │ zombie-1cecb9b5e0f9a14208f2fbefd9384490                                                            │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Provider                     │ kubernetes                                                                                         │
├──────────────────────────────┴────────────────────────────────────────────────────────────────────────────────────────────────────┤
│                                                         Node Information                                                          │
├──────────────────────────────┬────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Name                         │ alice                                                                                              │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Direct Link                  │ https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A34341#/explorer                           │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Prometheus Link              │ http://127.0.0.1:35537/metrics                                                                     │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Log Cmd                      │ kubectl logs -f alice -c alice -n zombie-1cecb9b5e0f9a14208f2fbefd9384490                          │
├──────────────────────────────┴────────────────────────────────────────────────────────────────────────────────────────────────────┤
│                                                         Node Information                                                          │
├──────────────────────────────┬────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Name                         │ bob                                                                                                │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Direct Link                  │ https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A44459#/explorer                           │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Prometheus Link              │ http://127.0.0.1:43841/metrics                                                                     │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Log Cmd                      │ kubectl logs -f bob -c bob -n zombie-1cecb9b5e0f9a14208f2fbefd9384490                              │
├──────────────────────────────┴────────────────────────────────────────────────────────────────────────────────────────────────────┤
│                                                         Node Information                                                          │
├──────────────────────────────┬────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Name                         │ charlie                                                                                            │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Direct Link                  │ https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A42069#/explorer                           │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Prometheus Link              │ http://127.0.0.1:42675/metrics                                                                     │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Log Cmd                      │ kubectl logs -f charlie -c charlie -n zombie-1cecb9b5e0f9a14208f2fbefd9384490                      │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Parachain ID                 │ 1000                                                                                               │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ChainSpec Path               │ /tmp/zombie-1cecb9b5e0f9a14208f2fbefd9384490_-29755-WOCdKtq9zPGA/1000-rococo-local.json            │
└──────────────────────────────┴────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

</details>

### Verifying the Setup

Check if all zombienet pods were started successfully:

```bash
kubectl get pods --all-namespaces
```

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

### Accessing the Parachain

The parachain is available through the Polkadot.js Apps interface by clicking on this link:

[https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A42069#/explorer](https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A42069#/explorer)

This link will automatically connect to Charlie's node running on a local machine at port `42069`. The port is configured in `local-kube-testnet.toml` under `rpc_port` for Charlie's node.

## Checking the logs

At the end of the `zombienet` output you should see a table like so:
```
┌───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                                         Node Information                                                          │
├──────────────────────────────┬────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Name                         │ charlie                                                                                            │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Direct Link                  │ https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:42069#/explorer                                   │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Prometheus Link              │ http://127.0.0.1:44955/metrics                                                                     │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Log Cmd                      │ tail -f                                                                                            │
│                              │ /tmp/nix-shell.gQQj4Y/zombie-bcb786e1748ff0a6becd28289e1f70b9_-677866-G8ea9Qqs65DB/charlie.log     │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Parachain ID                 │ 1000                                                                                               │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ChainSpec Path               │ /tmp/nix-shell.gQQj4Y/zombie-bcb786e1748ff0a6becd28289e1f70b9_-677866-G8ea9Qqs65DB/1000-rococo-lo… │
└──────────────────────────────┴────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

We strongly recommend you check the logs for the collator (in this case Charlie), using a text editor of your choice.

> If the **Log Cmd** is shortened with `...`, try to search for the folder using the zombienet namespace, available at the top of the table.
> For example:
> ```bash
> $ ls /tmp | grep zombie-bcb786e1748ff0a6becd28289e1f70b9
> ```

After finding the `charlie.log` file, you should `grep` for errors relating to the extrinsic you just ran,
for example, if you ran a `market` extrinsic, like `add-balance`, you would run:

```bash
$ grep "ERROR.*runtime::market" charlie.log
```

Or, more generally:

```bash
$ grep "<LOG_LEVEL>.*runtime::<EXTRINSIC_PALLET>" charlie.log
```

Where `LOG_LEVEL` is one of:
* `DEBUG`
* `INFO`
* `WARN`
* `ERROR`

And the extrinsic pallet, is one of:
* `storage_provider`
* `market`
* `proofs`

> **Tip:** if you are running into the `AllProposalsInvalid` error,
> try searching for `insane deal` in the logs, you should find the cause faster!
>
> For example:
> ```bash
> $ grep "insane deal" -B 1 charlie.log
> 2024-11-14 13:24:24.019 ERROR tokio-runtime-worker runtime::market: [Parachain] deal duration too short: 100 < 288000
> 2024-11-14 13:24:24.019 ERROR tokio-runtime-worker runtime::market: [Parachain] insane deal: idx 0, error: ProposalError::DealDurationOutOfBounds
> ```
