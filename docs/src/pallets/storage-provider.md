# Storage Provider Pallet

## Table of Contents

- [Storage Provider Pallet](#storage-provider-pallet)
  - [Table of Contents](#table-of-contents)
  - [Overview](#overview)
  - [Usage](#usage)
    - [Registering storage providers](#registering-storage-providers)
    - [Modifying storage provider information](#modifying-storage-provider-information)
    - [Declaring storage faults](#declaring-storage-faults)
    - [Declaring storage faults recovered](#declaring-storage-faults-recovered)
  - [Storage fault slashing](#storage-fault-slashing)
    - [Fault Fee (FF)](#fault-fee-ff)
    - [Sector Penalty (SP)](#sector-penalty-sp)
    - [Termination Penalty (TP)](#termination-penalty-tp)
    - [State management for Storage Providers](#state-management-for-storage-providers)
  - [Sector sealing](#sector-sealing)
  - [Storage Provider Flow](#storage-provider-flow)
    - [Registration](#registration)
    - [Commit](#commit)
    - [Proof of Spacetime submission](#proof-of-spacetime-submission)
  - [Storage provider pallet hooks](#storage-provider-pallet-hooks)
  - [Extrinsics](#extrinsics)
    - [`register_storage_provider`](#register_storage_provider)
    - [`pre_commit_sector`](#pre_commit_sector)
    - [`prove_commit_sector`](#prove_commit_sector)
    - [`submit_windowed_post`](#submit_windowed_post)
    - [`declare_faults`](#declare_faults)
    - [`declare_faults_recovered`](#declare_faults_recovered)
  - [Events](#events)
  - [Errors](#errors)

## Overview

The `Storage Provider Pallet` handles the creation of storage providers and facilitates storage providers and client in creating storage deals. Storage providers must provide Proof of Spacetime and Proof of Replication to the `Storage Provider Pallet` in order to prevent the pallet impose penalties on the storage providers through [slashing](#storage-fault-slashing).

## Usage

### Registering storage providers

A storage provider indexes in the storage provider pallet itself when it starts up by calling the `create_storage_provider` extrinsic with it's `PeerId` as an argument. The public key will be extracted from the origin and is used to modify on-chain information and receive rewards. The `PeerId` is given by the storage provider so clients can use that to connect to the storage provider.

### Modifying storage provider information

The `Storage Provider Pallet` allows storage providers to modify their information such as changing the peer id, through `change_peer_id` and changing owners, through `change_owner_address`.

### Declaring storage faults

A storage provider can declare sectors as faulty, through the `declare_faults`, for any sectors that it cannot generate `WindowPoSt` proofs. A storage provider has to declare the sector as faulty **before** the challenge window. Until the sectors are recovered they will be masked from proofs in subsequent proving periods.

### Declaring storage faults recovered

After a storage provider has declared some sectors as faulty, it can recover those sectors. The storage provider can use the `declare_faults_recovered` method to set the sectors it previously declared as faulty to recovering.

## Storage fault slashing

Storage Fault Slashing refers to a set of penalties that storage providers may incur if they fail to maintain sector reliability or choose to voluntarily exit the network. These penalties include Fault Fees, Sector Penalties, and Termination Fees. Below is a detailed explanation of each type of penalty.

### Fault Fee (FF)

- **Description**: A penalty incurred by a storage provider for each day that a sector is offline.
- **Rationale**: Ensures that storage providers maintain high availability and reliability of their committed data.

### Sector Penalty (SP)

- **Description**: A penalty incurred by a storage provider for a sector that becomes faulted without being declared as such before a WindowPoSt (Proof-of-Spacetime) check.
- **Rationale**: Encourages storage providers to promptly declare any faults to avoid more severe penalties.
- **Details**: If a fault is detected during a WindowPoSt check, the sector will incur an SP and will continue to incur a FF until the fault is resolved.

### Termination Penalty (TP)

- **Description**: A penalty incurred when a sector is either voluntarily or involuntarily terminated and removed from the network.
- **Rationale**: Discourages storage providers from arbitrarily terminating sectors and ensures they fulfill their storage commitments.

By implementing these penalties, storage providers are incentivised to maintain the reliability and availability of the data they store. This system of Storage Fault Slashing helps maintain the integrity and reliability of our decentralized storage network.

### State management for Storage Providers

In our parachain, the state management for all storage providers is handled collectively, unlike Filecoin, which manages the state for individual storage providers.

## Sector sealing

Before a sector can be used, the storage provider must seal the sector, which involves encoding the data in the sector to prepare it for the proving process.

- **Unsealed Sector**: An unsealed sector is a sector containing raw data that has not yet been sealed.
- **UnsealedCID (CommD)**: The root hash of the unsealed sector’s Merkle tree, also referred to as CommD or "data commitment."
- **Sealed Sector**: A sector that has been encoded and prepared for the proving process.
- **SealedCID (CommR)**: The root hash of the sealed sector’s Merkle tree, also referred to as CommR or "replica commitment."

By sealing sectors, storage providers ensure that data is properly encoded and ready for the proof-of-storage process, maintaining the integrity and security of the stored data in the network.

Sealing a sector using Proof-of-Replication (PoRep) is a computation-intensive process that results in a unique encoding of the sector. Once the data is sealed, storage providers follow these steps:

- **Generate a Proof**: Create a proof that the data has been correctly sealed.
- **Run a SNARK on the Proof**: Compress the proof using a Succinct Non-interactive Argument of Knowledge (SNARK).
- **Submit the Compressed Proof:** Submit the result of the compression to the blockchain as certification of the storage commitment.

## Storage Provider Flow

### Registration

The first thing a storage provider must do is register itself by calling `storage_provider.create_storage_provider(peer_id: PeerId, window_post_proof_type: RegisteredPoStProof)`. At this point there are no funds locked in the storage provider pallet. The next step is to place storage market asks on the market, this is done through the market pallet. After that the storage provider needs to make deals with clients and begin filling up sectors with data. When they have a full sector they should seal the sector.

### Commit

When the storage provider has completed their first seal, they should post it to the storage provider pallet by calling `storage_provider.pre_commit_sector(sectors: SectorPreCommitInfo)`. If the storage provider had zero committed sectors before this call, this begins their proving period. The proving period is a fixed amount of time in which the storage provider must submit a Proof of Space Time to the network.
During this period, the storage provider may also commit to new sectors, but they will not be included in proofs of space time until the next proving period starts. During the prove commit call, the storage provider pledges some collateral in case they fail to submit their PoSt on time.

### Proof of Spacetime submission

When the storage provider has completed their PoSt, they must submit it to the network by calling `storage_provider.submit_windowed_post(deadline: u64, partitions: Vec<u64>, proofs: Vec<PostProof>)`. There are two different types of submissions:

- **Standard Submission**: A standard submission is one that makes it on-chain before the end of the proving period.
- **Penalize Submission**:A penalized submission is one that makes it on-chain after the end of the proving period, but before the generation attack threshold. These submissions count as valid PoSt submissions, but the miner must pay a penalty for their late submission. See [storage fault slashing](#storage-fault-slashing).

## Storage provider pallet hooks

Substrate pallet hooks execute some actions when certain conditions are met. We use these hooks, when a block finalizes, to check if storage providers are up to date with their proofs. If a proof needs to be submitted but isn't the storage provider pallet will penalize the storage provider accordingly [slash](#storage-fault-slashing) their collateral that the locked up during the [pre commit section](#commit).

## Extrinsics

### `register_storage_provider`

Storage Provider registration is the first extrinsic that any storage provider has to call, without being registered, the other extrinsics will return an error.

Before a storage provider can register, they need to set up a [PeerId](https://docs.libp2p.io/concepts/fundamentals/peers/#peer-id). This [PeerId](https://docs.libp2p.io/concepts/fundamentals/peers/#peer-id)is used in the p2p network to connect to the storage provider.

| Name                     | Description                          |
| ------------------------ | ------------------------------------ |
| `peer_id`                | libp2p ID                            |
| `window_post_proof_type` | Proof type the storage provider uses |

#### <a class="header" id="register_storage_provider.example" href="#register_storage_provider.example">Example</a>

Registering a storage provider with keypair `//Alice` and peer ID `alice`

```bash
storagext-cli --sr25519-key "//Alice" storage-provider register alice
```

### `pre_commit_sector`

After a deal has been published the storage provider needs to pre-commit the sector information to the chain. Sectors are not valid after pre-commit, the sectors need to be proven first.

| Name            | Description                                                               |
| --------------- | ------------------------------------------------------------------------- |
| `seal_proof`    | Seal proof type this storage provider is using [^note]                    |
| `sector_number` | The sector number that is being pre-committed                             |
| `sealed_cid`    | Commitment of replication, more info in [sector sealing](#sector-sealing) |
| `deal_ids`      | Deal IDs to be pre-committed, from `publish_storage_deals`                |
| `expiration`    | Expiration block of the pre-committed sector                              |
| `unsealed_cid`  | Commitment of data, more info in [sector sealing](#sector-sealing)        |

<div class="warning">
Sectors are not valid after pre-commit, the sectors need to be proven first.
</div>

[^note]: Only once seal proof type supported at the moment, `2KiB`.

#### <a class="header" id="pre_commit_sector.example" href="#pre_commit_sector.example">Example</a>

Storage provider `//Alice` pre-committing a sector number 1, with a single deal ID 0.

```bash
storagext-cli --sr25519-key "//Alice" storage-provider pre-commit @pre-commit-sector.json
```

Where `pre-commit-sector.json` is a file with contents similar to:

```json
{
    "sector_number": 1,
    "sealed_cid": "bafk2bzaceajreoxfdcpdvitpvxm7vkpvcimlob5ejebqgqidjkz4qoug4q6zu",
    "deal_ids": [0],
    "expiration": 100,
    "unsealed_cid": "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
    "seal_proof": "StackedDRG2KiBV1P1"
}
```

### `prove_commit_sector`

After pre-committing some new sectors the storage provider needs to supply a Proof-of-Replication (PoRep)] for these sectors. More info about the PoRep can be found in the [sector sealing section](#sector-sealing).[^note]


| Name            | Description                                     |
| --------------- | ----------------------------------------------- |
| `sector_number` | The sector number that is being prove-committed |
| `proof`         | The hex-encoded bytes of a proof                |

[^note]: At the moment any non-zero length proof is accepted for PoRep.

#### <a class="header" id="prove_commit_sector.example" href="#prove_commit_sector.example">Example</a>

This example follows up on the pre-commit example. Storage provider `//Alice` is prove committing sector number 1.

```bash
storagext-cli --sr25519-key "//Alice" storage-provider prove-commit @prove-commit-sector.json
```

Where `prove-commit-sector.json` is a file with contents similar to:

```json
{
    "sector_number": 1,
    "proof": "1230deadbeef"
}
```

### `submit_windowed_post`

A storage provider needs to periodically submit a (Proof-of-Spacetime (PoSt))[#proof-of-spacetime-submission] to prove that they are still storing the data they promised. Multiple proofs can be submitted at once.

| Name          | Description                                                               |
| ------------- | ------------------------------------------------------------------------- |
| `deadline`    | The deadline index which the submission targets                           |
| `partitions`  | The partition being proven                                                |
| `post_proof`  | The proof type, should be consistent with the proof type for registration |
| `proof_bytes` | The proof submission, to be checked in the storage provider pallet.       |

#### <a class="header" id="submit_windowed_post.example" href="#submit_windowed_post.example">Example</a>

Storage provider `//Alice` submitting proof for deadline 0, partition 0.

```bash
storagext-cli --sr25519-key "//Alice" storage-provider submit-windowed-post @submit-windowed-post.json
```

Where `submit-windowed-post.json` is a file with contents similar to:

```json
{
    "deadline": 0,
    "partition": [0],
    "proof": {
        "post_proof": "2KiB",
        "proof_bytes": "1230deadbeef"
    }
}
```

### `declare_faults`

A storage provider can declare faults when they know that they cannot submit PoSt on time to prevent to get penalized. Faults have an expiry of 42 days. If the faults are not recovered before this time, the sectors will be terminated. Multiple faults can be declared at once.

`declare_faults` can take in multiple fault declarations:

| Name     | Description                    |
| -------- | ------------------------------ |
| `faults` | An array of fault declarations |

Where the fault declarations contain:

| Name        | Description                                                        |
| ----------- | ------------------------------------------------------------------ |
| `deadline`  | The deadline to which the faulty sectors are assigned              |
| `partition` | Partition index within the deadline containing the faulty sectors. |
| `sectors`   | Sectors in the partition being declared faulty                     |

#### <a class="header" id="declare_faults.example" href="#declare_faults.example">Example</a>

Storage provider `//Alice` declaring faults on deadline 0, partition 0, sector 0.

```bash
storagext-cli --sr25519-key "//Alice" storage-provider declare-faults @fault-declaration.json
```

Where `fault-declaration.json` is a file with contents similar to:

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

### `declare_faults_recovered`

After declaring sectors as faulty a storage provider can recover the sectors. If the system has marked some sectors as faulty, due to a missing PoSt, the storage provider needs to recover the faults. Faults are not fully recovered until the storage provider submits a valid PoSt after the `declare_faults_recovered` extrinsic.

`declare_faults_recovered` can take in multiple fault recoveries:

| Name         | Description                  |
| ------------ | ---------------------------- |
| `recoveries` | An array of fault recoveries |

Where the fault recoveries contain:

| Name        | Description                                                          |
| ----------- | -------------------------------------------------------------------- |
| `deadline`  | The deadline to which the recovered sectors are assigned             |
| `partition` | Partition index within the deadline containing the recovered sectors |
| `sectors`   | Sectors in the partition being declared recovered                    |

#### <a class="header" id="declare_faults_recovered.example" href="#declare_faults_recovered.example">Example</a>

Storage provider `//Alice` declaring recoveries on deadline 0, partition 0, sector 0.

```bash
storagext-cli --sr25519-key "//Alice" storage-provider declare-faults-recovered @fault-declaration.json
```

Where `fault-declaration.json` is a file with contents similar to:

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

## Events

The Storage Provider Pallet emits the following events:

- `StorageProviderRegistered` - Indicates that a new storage provider has been registered.
  - `owner` - SS58 address of the storage provider.
  - `info` - The static information about the new storage provider. This information includes:
    - `peer_id` - Libp2p identity that should be used when connecting to the storage provider.
    - `window_post_proof_type` - The proof type used by the storage provider for sealing sectors.
    - `sector_size` - Amount of space in each sector committed to the network by the storage provider.
    - `window_post_partition_sectors` - The number of sectors in each Window PoSt partition (proof).
- `SectorPreCommitted` - A storage provider has pre-committed some new sector after publishing some new deal.
  - `owner` - SS58 address of the storage provider.
  - `sector` - The sector number being pre-committed.
- `SectorProven` - A storage provider has proven a sector that they previously pre-committed.
  - `owner` - SS58 address of the storage provider.
  - `sector_number` - The sector number that was proven.
- `SectorSlashed` - A sector that was previously pre-committed, but not proven, has been slashed by the system because it has expired.
  - `owner` - SS58 address of the storage provider.
  - `sector_number` - The sector number that has been slashed because of expiry.
- `ValidPoStSubmitted` - A valid PoSt has been submitted by a storage provider.
  - `owner` - SS58 address of the storage provider.
- `FaultsDeclared` - A storage provider has declared some sectors as faulty.
  - `owner` - SS58 address of the storage provider.
  - `faults` - An array with information about the fault declarations. This information includes:
    - `deadline` - The deadline to which the faulty sectors are assigned.
    - `partition` - Partition number within the deadline containing the faulty sectors.
    - `sectors` - Sectors in the partition being declared as faulty.
- `FaultsRecovered` - A storage provider has recovered some sectors previously declared as faulty.
  - `owner` - SS58 address of the storage provider.
  - `recoveries` - An array with information about the fault recoveries. This information includes:
    - `deadline` - The deadline to which the recovered sectors are assigned.
    - `partition` - Partition number within the deadline containing the recovered sectors.
    - `sectors` - Sectors in the partition being declared as recovered.
- `PartitionFaulty` - The system has detected that a storage provider has not submitted their PoSt on time and has marked some sectors as faulty.
  - `owner` - SS58 address of the storage provider.
  - `partition` - Partition number for which the PoSt was missed.
  - `sectors` - The sectors in the partition that were declared as faulty by the system.

## Errors

The Storage Provider Pallet actions can fail with following errors:

- `StorageProviderExists` - A storage provider is already registered and tries to register again.
- `StorageProviderNotFound` - This error is emitted by all extrinsics except registration in the storage provider pallet when a storage provider tries to call an extrinsic without registering first.
- `InvalidSector` - This error can be emitted when:
  - A storage provider supplies a sector number during pre-commit that exceeds the maximum amount of sectors.
  - A storage provider supplies a sector number during prove commit that exceeds the maximum amount of sectors.
- `InvalidProofType` - This error can be emitted when:
  - A storage provider submits a seal proof type during pre-commit that is different than the one configured during registration.
  - During a prove commit extrinsic the proof type that the storage provider has supplied in invalid.
  - A storage provider submits a windowed PoSt proof type that is different than the one configured during registration.
- `NotEnoughFunds` - Emitted when a storage provider does not have enough funds for the pre-commit deposit.
- `SectorNumberAlreadyUsed` - A storage provider tries to pre-commit a sector number that has already been used.
- `ExpirationBeforeActivation` - A storage provider tries to pre-commit a sector where the expiration of that sector is before the activation.
- `ExpirationTooSoon` - A storage provider tries to pre-commit a sector with a total lifetime that is less that `MinSectorExpiration`.
- `ExpirationTooLong` - A storage provider tries to pre-commit a sector with an expiration that exceeds `MaxSectorExpirationExtension`.
- `MaxSectorLifetimeExceeded` - A storage provider tries to pre-commit a sector with a total lifetime that exceeds `SectorMaximumLifetime`.
- `InvalidCid` - Emitted when a storage provider submits and invalid  unsealed CID when trying to pre-commit a sector.
- `ProveCommitAfterDeadline` - A storage provider has tried to prove a previously pre-committed sector after the proving deadline.
- `PoStProofInvalid` - A proof that was submitted by the storage provider is invalid. Currently this error is emitted when the proof length is 0.
- `InvalidUnsealedCidForSector` - This error is emitted when the declared unsealed_cid for pre_commit is different from the one calculated by the system.
- `FaultDeclarationTooLate` - A fault declaration was submitted after the fault declaration cutoff. The fault declaration can be submitted after the upcoming deadline is closed.
- `FaultRecoveryTooLate` - A fault recovery was submitted after the fault recovery cutoff. The fault recovery can be submitted after the upcoming deadline is closed.
- `DeadlineError` - An error was encountered in the deadline module. If you encounter this error please report an issue as this is a programmer error.
- `PartitionError` - An error was encountered in the partition module. If you encounter this error please report an issue as this is a programmer error.
- `StorageProviderError` - An error was encountered in the storage provider module. If you encounter this error please report an issue as this is a programmer error.
- `SectorMapError` - An error was encountered in the sector map module. These errors can be:
- `CouldNotActivateSector` - Failure during prove commit when trying to convert a previously pre-committed sector due to a programming error. Please report an issue if you receive this error.
- `CouldNotVerifySectorForPreCommit` - Failure during pre-commit due to the commd calculation failing due to a programming error. Please report an issue if you receive this error.
- `SlashingFailed` - Slashing of funds fails due to a programmer error. Please report an issue if you receive this error.
- `ConversionError` - Due to a programmer error. Please report an issue if you receive this error.
