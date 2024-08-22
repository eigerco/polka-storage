# CLI examples

This document contains examples for transactions for storage providers.

## Table of Contents

- [Storage Provider registration](#storage-provider-registration)
  - [Registration using the CLI](#registration-using-the-cli)
- [Add market balance](#add-market-balance)
  - [Adding market balance using the CLI](#adding-market-balance-using-the-cli)
- [Publish storage deals](#publish-storage-deals)
  - [Publishing storage deals using the CLI](#publishing-storage-deals-using-the-cli)
- [Pre commit sector](#pre-commit-sector)
  - [Pre committing sector using the CLI](#pre-committing-sector-using-the-cli)
- [Prove commit sector](#prove-commit-sector)
  - [Prove committing sector using the CLI](#prove-committing-sector-using-the-cli)
- [Submit windowed Proof-of-Spacetime](#submit-windowed-proof-of-spacetime)
  - [Submitting windowed PoSt using the CLI](#submitting-windowed-post-using-the-cli)
- [Declare faults](#declare-faults)
  - [Declaring faults using the CLI](#declaring-faults-using-the-cli)
- [Declare faults recovered](#declare-faults-recovered)
  - [Declaring fault recovered using the CLI](#declaring-fault-recovered-using-the-cli)


## Storage Provider registration

Storage Provider registration is the first extrinsic that any storage provider should call.

> [!IMPORTANT]
> All other storage provider extrinsics will be rejected if the storage provider is not registered.

Before a storage provider can register, they need to set up a [PeerId](todo: link to peer id). This PeerId is used in the p2p network to connect to the storage provider.

### Registration using the CLI

<details>

Registering as a new storage provider using the CLI can be done in a single CLI call.

Command:

```bash
storagext-cli --sr25519-key <keypair> storage-provider register <PeerId>
```

</details>

## Add market balance

Any user, storage client or storage provider can add balance to the market pallet. Storage clients need to add balance to setup a storage deal and storage providers need to add balance as collateral for the storage deals. It is possible to add more balance than a storage deal needs to save on gas costs.

> [!NOTE]
> Adding balance to the market pallet is a prerequisite for setting up storage deals.

### Adding market balance using the CLI

<details>

Command:

```bash
storagext-cli --sr25519-key <keypair> market add-balance <amount>
```

</details>

## Publish storage deals

Once a deal has been reached between the storage provider and the storage client, the deal needs to be published to the market pallet. The deals are passed into the CLI in JSON format and support the publishing of multiple deals at once. For this example we will use a single deal.

### Publishing storage deals using the CLI

<details>

JSON example `deal.json`:

```json
[
    {
        "piece_cid": "bafk2bzacecg3xxc4f2ql2hreiuy767u6r72ekdz54k7luieknboaakhft5rgk",
        "piece_size": 1,
        "client": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
        "provider": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
        "label": "dead",
        "start_block": 30,
        "end_block": 55,
        "storage_price_per_block": 1,
        "provider_collateral": 1,
        "state": "Published"
    }
]
```

Command:

```bash
storagext-cli --sr25519-key <storage_provider_keypair> market publish-storage-deals --client-sr25519-key <storage_client_keypair> @deal.json
```

</details>

## Pre commit sector

After a deal has been published the storage provider needs to add the sector information to the parachain. This can be done using the `pre_commit_sector` extrinsic.

> [!NOTE]
> Sectors are not valid after pre-commit, the sectors need to be proven first.

### Pre committing sector using the CLI

<details>

JSON example `pre-commit-sector.json`:

```json
{
    "sector_number": 1,
    "sealed_cid": "bafk2bzaceajreoxfdcpdvitpvxm7vkpvcimlob5ejebqgqidjkz4qoug4q6zu",
    "deal_ids": [0],
    "expiration": 100,
    "unsealed_cid": "bafk2bzaceajreoxfdcpdvitpvxm7vkpvcimlob5ejebqgqidjkz4qoug4q6zu",
    "seal_proof": "StackedDRG2KiBV1P1"
}
```

Command:

```bash
storagext-cli --sr25519-key <keypair> storage-provider pre-commit @pre-commit-sector.json
```

</details>

## Prove commit sector

After pre-committing some new sectors the storage provider needs to supply proof for these sectors. The `prove_commit_sector` extrinsic is used for this.

### Prove committing sector using the CLI

<details>

JSON example `prove-commit-sector.json`

```json
{
    "sector_number": 1,
    "proof": "1230deadbeef"
}
```

Command:

```bash
storagext-cli --sr25519-key <keypair> storage-provider prove-commit @prove-commit-sector.json
```

</details>

## Submit windowed Proof-of-Spacetime

A storage provider needs to periodically submit a Proof-of-Spacetime (PoSt) to prove that they are still storing the data they promised. The storage provider uses the `submit_windowed_post` extrinsic for this. Multiple proofs can be submitted at once. For the example we are using a single proof.

### Submitting windowed PoSt using the CLI

<details>

JSON example `submit-windowed-post.json`:

```json
{
    "deadline": 0,
    "partition": 0,
    "proof": {
        "sector_number": 1,
        "proof_bytes": "1230deadbeef"
    }
}
```

Command:

```bash
storagext-cli storage-provider submit-windowed-post @submit-windowed-post.json
```

</details>

## Declare faults

A storage provider can declare faults when they know that they cannot submit PoSt on time to prevent to get penalized. Faults have an expiry of 42 days. If the faults are not recovered before this time, the sectors will be terminated. Multiple faults can be declared at once.

### Declaring faults using the CLI

<details>

JSON example `fault-declaration.json`:

```json
[
    {
        "deadline": 0,
        "partition": 0,
        "sectors": [
            1
        ]
    }
]
```

Command:

```bash
storagext-cli storage-provider declare-faults @fault-declaration.json
```

</details>

## Declare faults recovered

After declaring sectors as faulty a storage provider can recover the sectors by using the `declare_faults_recovered` extrinsic. If the system has marked some sectors as faulty, due to a missing PoSt, the storage provider needs to recover the faults.

> [!IMPORTANT]
> Faults are not fully recovered until the storage provider submits a valid PoSt after the `declare_faults_recovered` extrinsic.

### Declaring fault recovered using the CLI

<details>

JSON example `fault-declaration.json`:

```json
[
    {
        "deadline": 0,
        "partition": 0,
        "sectors": [
            1
        ]
    }
]
```

Command:

```bash
storagext-cli storage-provider declare-faults-recovered @fault-declaration.json
```

</details>
