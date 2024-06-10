# Storage Provider Pallet

## Overview

The `Storage Provider Pallet` handles the creation of storage providers and facilitates storage providers and client in creating storage deals.

## Usage

### Indexing storage providers

A storage provider indexes in the storage provider pallet itself when it starts up by calling the `create_storage_provider` extrinsic with it's `PeerId` as an argument. The public key will be extracted from the origin and is used to modify on-chain information and receive rewards. The `PeerId` is given by the storage provider so clients can use that to connect to the storage provider.

### Modifying storage provider information

The `Storage Provider Pallet` allows storage providers to modify their information such as changing the peer id, through `change_peer_id` and changing owners, through `change_owner_address`.

## State management for Storage Providers

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

## Data structures

### Proof of spacetime

Proof of spacetime indicates the version and the sector size of the proof. This type is used by the Storage Provider when initially starting up to indicate what PoSt version it will use to submit Window PoSt proof.

```rust
pub enum RegisteredPoStProof {
    StackedDRGWinning2KiBV1,
    StackedDRGWinning8MiBV1,
    StackedDRGWinning512MiBV1,
    StackedDRGWinning32GiBV1,
    StackedDRGWinning64GiBV1,
    StackedDRGWindow2KiBV1P1,
    StackedDRGWindow8MiBV1P1,
    StackedDRGWindow512MiBV1P1,
    StackedDRGWindow32GiBV1P1,
    StackedDRGWindow64GiBV1P1,
    Invalid(i64),
}
```

The `SectorSize` indicates one of a set of possible sizes in the network.

```rust
#[repr(u64)]
pub enum SectorSize {
    _2KiB
    _8MiB,
    _512MiB,
    _32GiB,
    _64GiB,
}
```

The `PoStProof` is the proof of spacetime data that is stored on chain

```rust
pub struct PoStProof {
    pub post_proof: RegisteredPoStProof,
    pub proof_bytes: Vec<u8>,
}
```

### Proof of replication

Proof of replication is used when a Storage Provider wants to store data on behalf of a client and receives a piece of client data. The data will first be placed in a sector after which that sector is sealed by the storage provider. Then a unique encoding, which serves as proof that the Storage Provider has replicated a copy of the data they agreed to store, is generated. Finally, the proof is compressed and submitted to the network as certification of storage.

```rust
/// This type indicates the seal proof type which defines the version and the sector size
pub enum RegisteredSealProof {
    StackedDRG2KiBV1,
    StackedDRG512MiBV1,
    StackedDRG8MiBV1,
    StackedDRG32GiBV1,
    StackedDRG64GiBV1,
    StackedDRG2KiBV1P1,
    StackedDRG512MiBV1P1,
    StackedDRG8MiBV1P1,
    StackedDRG32GiBV1P1,
    StackedDRG64GiBV1P1,
    StackedDRG2KiBV1P1_Feat_SyntheticPoRep,
    StackedDRG512MiBV1P1_Feat_SyntheticPoRep,
    StackedDRG8MiBV1P1_Feat_SyntheticPoRep,
    StackedDRG32GiBV1P1_Feat_SyntheticPoRep,
    StackedDRG64GiBV1P1_Feat_SyntheticPoRep,
    Invalid(i64),
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