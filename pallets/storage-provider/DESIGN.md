# Storage Provider Pallet

- [Storage Provider Pallet](#storage-provider-pallet)
  - [Overview](#overview)
    - [Constants \& Terminology](#constants--terminology)
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
    - [Static information about a Storage Provider](#static-information-about-a-storage-provider)
  - [Sector sealing](#sector-sealing)
  - [Data structures](#data-structures)
    - [Proof of Spacetime](#proof-of-spacetime)
    - [Proof of Replication](#proof-of-replication)

## Overview

The `Storage Provider Pallet` handles the creation of storage providers and facilitates storage providers and client in creating storage deals. Storage providers must provide Proof of Spacetime and Proof of Replication to the `Storage Provider Pallet` in order to prevent the pallet impose penalties on the storage providers through [slashing](#storage-fault-slashing).

### Constants & Terminology

- **Sector**: The sector is the default unit of storage that providers put in the network. A sector is a contiguous array of bytes that a storage provider puts together, seals, and performs Proofs of Spacetime on. Storage providers store data on the network in fixed-size sectors.
- **Partition**: A group of 2349 sectors proven simultaneously.
- **Proving Period**: The average period for proving all sectors maintained by a provider (currently set to 24 hours).
- **Deadline**: One of the multiple points during a proving period when proofs for some partitions are due.
- **Challenge Window**: The period immediately before a deadline during which a challenge can be generated by the chain and the requisite proofs computed.
- **Provider Size**: The amount of proven storage maintained by a single storage provider.

## Usage

> [!NOTE]
> For more information about the storage provider pallet API check out [the API docs](./API.md)

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

### Static information about a Storage Provider

The below struct and its fields ensure that all necessary static information about a Storage provider is encapsulated, allowing for efficient management and interaction within the parachain.

```rust
pub struct StorageProviderInfo<AccountId, PeerId> {
    /// Libp2p identity that should be used when connecting to this Storage Provider
    pub peer_id: PeerId,

    /// The proof type used by this Storage provider for sealing sectors.
    /// Rationale: Different StorageProviders may use different proof types for sealing sectors. By storing
    /// the `window_post_proof_type`, we can ensure that the correct proof mechanisms are applied and verified
    /// according to the provider's chosen method. This enhances compatibility and integrity in the proof-of-storage
    /// processes.
    pub window_post_proof_type: RegisteredPoStProof,

    /// Amount of space in each sector committed to the network by this Storage Provider
    /// 
    /// Rationale: The `sector_size` indicates the amount of data each sector can hold. This information is crucial
    /// for calculating storage capacity, economic incentives, and the validation process. It ensures that the storage
    /// commitments made by the provider are transparent and verifiable.
    pub sector_size: SectorSize,

    /// The number of sectors in each Window PoSt partition (proof).
    /// This is computed from the proof type and represented here redundantly.
    /// 
    /// Rationale: The `window_post_partition_sectors` field specifies the number of sectors included in each
    /// Window PoSt proof partition. This redundancy ensures that partition calculations are consistent and
    /// simplifies the process of generating and verifying proofs. By storing this value, we enhance the efficiency
    /// of proof operations and reduce computational overhead during runtime.
    pub window_post_partition_sectors: u64,
}
```

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

## Data structures

### Proof of Spacetime

> [!NOTE]
> For more information about proofs check out the [proof of storage docs](./PROOF-OF-STORAGE.md)

Proof of Spacetime indicates the version and the sector size of the proof. This type is used by the Storage Provider when initially starting up to indicate what PoSt version it will use to submit Window PoSt proof.

```rust
pub enum RegisteredPoStProof {
    StackedDRGWindow2KiBV1P1,
}
```

The `SectorSize` indicates one of a set of possible sizes in the network.

```rust
#[repr(u64)]
pub enum SectorSize {
    _2KiB,
}
```

The `PoStProof` is the proof of spacetime data that is stored on chain

```rust
pub struct PoStProof {
    pub post_proof: RegisteredPoStProof,
    pub proof_bytes: Vec<u8>,
}
```

### Proof of Replication

> [!NOTE]
> For more information about proofs check out the [proof of storage docs](./PROOF-OF-STORAGE.md)

Proof of Replication is used when a Storage Provider wants to store data on behalf of a client and receives a piece of client data. The data will first be placed in a sector after which that sector is sealed by the storage provider. Then a unique encoding, which serves as proof that the Storage Provider has replicated a copy of the data they agreed to store, is generated. Finally, the proof is compressed and submitted to the network as certification of storage.

```rust
/// This type indicates the seal proof type which defines the version and the sector size
pub enum RegisteredSealProof {
    StackedDRG2KiBV1P1,
}
```

The unique encoding created during the sealing process is generated using the sealed data, the storage provider who seals the data and the time at which the data was sealed.

```rust
/// This type is passed into the pre commit function on the storage provider pallet
pub struct SectorPreCommitInfo {
    pub seal_proof: RegisteredSealProof,
    pub sector_number: SectorNumber,
    pub sealed_cid: Cid,
    pub expiration: u64,
}
```