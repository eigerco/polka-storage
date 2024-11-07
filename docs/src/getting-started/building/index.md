# Building

The following chapters will cover how to build the Polka Storage parachain using multiple methods.

<div class="warning">
Quick reminder that Windows is not part of the supported operating systems.
As such, the following guides <b>have not been tested</b> on Windows.
</div>


We'll be building 5 artifacts:

* `polka-storage-node` — the Polka Storage Polkadot parachain node.
* `polka-storage-provider-server` — the Polka Storage Storage Provider,
  responsible for accepting deals, storing files and executing storage proofs.
* `polka-storage-provider-client` — the Polka Storage Storage Provider client,
  this CLI tool has several utilities to interact with the Storage Provider server,
  such as proposing and publishing deals, as well as some wallet utilities and proof demos.
* `mater-cli` — the Mater CLI enables you to convert files into CARv2 archives,
  an essential part of preparing files for submission to the network.
* `storagext-cli` — the Storagext CLI is a lower-level tool to manually run the Polka Storage extrinsics.

To build these artifacts, we provide two main methods:

* [Building from source](./source.md)
* [Building with Docker](./docker.md)
