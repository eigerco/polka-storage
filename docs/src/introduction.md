# Introduction

Welcome to the Polka Storage project book. This document is a work in progress and will be constantly updated.

This project aims to build a native storage network for Polkadot.

We've now completed Phase 2 and have started work on Phase 3.

**During [**Phase 2**](https://polkadot.polkassembly.io/referenda/1150), we have implemented:**

- Storage Provider Pallet
  - [`terminate_sectors`](./architecture/pallets/storage-provider.md#terminate_sectors)
  - `process_early_terminations` (internal)
  - Batch support
    - [Pre-commit](./architecture/pallets/storage-provider.md#pre_commit_sectors)
    - [Prove-commit](./architecture/pallets/storage-provider.md#prove_commit_sectors)
- [Storage Provider Server](./architecture/polka-storage-provider-server.md)
  - [File upload](./getting-started/demo-file-store.md)
  - [Proving Pipeline](./architecture/polka-storage-provider-server.md#sealing-pipeline)
    - Pre-commit
    - PoRep
- On-chain proof validation
  - Proof of Replication
  - Proof of Spacetime

We also present a complete [real-world scenario](./getting-started/demo-file-store.md) in which a [Storage Provider](./glossary.md#storage-provider) and a [Storage User](./glossary.md#storage-user) negotiate a deal, perform all the steps necessary to start the storage and then receive rewards (or punishments) for making it happen.

**The Polka Storage project currently provides:**

Dedicated CLIs

- [Polka Storage Provider CLI](./storage-provider-cli/index.md)
  - [`polka-storage-provider-server`](./storage-provider-cli/server.md) to launch the Storage Provider server.
  - [`polka-storage-provider-client`](./storage-provider-cli/client/index.md) to manage the wallet, propose & publish deals and do proof demos.
- [`mater-cli`](./mater-cli/index.md) to convert or extract CARv2 files.
- [`storagext-cli`](./storagext-cli/index.md) to interact **directly** with the parachain — watch out, this is a low-level tool!

Pallets:

- [Storage Provider](./architecture/pallets/storage-provider.md)
- [Market](./architecture/pallets/market.md)
- [Proofs](./architecture/pallets/proofs.md)
- [Randomness](./architecture/pallets/randomness.md)

<p>
    <img
        src="images/showcase/client_upload/showcase.gif"
        alt="Polka Storage Client Upload">
</p>

**During  [Phase 1](https://polkadot.polkassembly.io/referenda/494), we implemented the following:**

- Keeping track of [Storage Providers](./glossary.md#storage-provider),
- [Publishing](./architecture/pallets/market.md#publish_storage_deals) Market Deals on-chain,
- [Investing](./architecture/pallets/market.md#add_balance) tokens into the Storage Market,
- [Receiving](./architecture/pallets/market.md#settle_deal_payments) funds after completing a deal,
- [Committing](./architecture/pallets/storage-provider.md#pre_commit_sectors) to the Storage and [Proving](./architecture/pallets/storage-provider.md#prove_commit_sectors) the storage,
- [Declaring](./architecture/pallets/storage-provider.md#prove_commit_sectors) failures to deliver committed storage and [Recovering](./architecture/pallets/storage-provider.md#declaring-storage-faults-recovered) from it,
- [Continuously proving](./architecture/pallets/storage-provider.md#submit_windowed_post) that the promise of storage has been kept up [PoSt proof](./glossary.md#proofs),
- [Terminating](./architecture/pallets/storage-provider.md#terminate_sectors) sectors by the storage provider,
- [Punishing](./architecture/pallets/storage-provider.md#events) for failing to provide storage.

We present a demo on how to [store a file](./getting-started/demo-file-store.md), where a [Storage Provider](./glossary.md#storage-provider) and a [Storage User](./glossary.md#storage-user) negotiate a deal and perform all the steps necessary to start the file storage. We cover the details behind proving a file in a [separate demo](./getting-started/demo-file-prove.md).

**More information available about the project's genesis in:**

- OpenGov Referendum - Part 1 — <https://polkadot.polkassembly.io/referenda/494>
- OpenGov Referendum - Part 2 — <https://polkadot.polkassembly.io/referenda/1150>
- Research Report — <https://github.com/eigerco/polkadot-native-storage/blob/main/doc/report/polkadot-native-storage-v1.0.0.pdf>
- Weekly dev updates <https://forum.polkadot.network/t/polkadot-native-storage-updates/7021>

---

<p>
    <a href="https://eiger.co">
        <img
            src="images/logo.svg"
            alt="Eiger Oy"
            style="height: 50px; display: block; margin-left: auto; margin-right: auto; width: 50%;">
    </a>
</p>
