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

### State Structure

The State structure maintains all the necessary information about the storage providers. This structure includes details about funds, sectors, and deadlines.

```rust
#[derive(Encode, Decode, Default, Clone, PartialEq, Debug)]
pub struct ProviderInfo {
    // Contains static information about the storage provider.
    pub info: Cid,
    /// Total funds locked as pre_commit_deposit
    pub pre_commit_deposits: u128,
    /// Total rewards and added funds locked in vesting table
    pub locked_funds: u128,
    /// Sum of initial pledge requirements of all active sectors
    pub initial_pledge: u128,
    /// Sectors that have been pre-committed but not yet proven
    pub pre_committed_sectors: Cid,
    /// Allocated sector IDs.
    pub allocated_sectors: Cid,
    /// Information for all proven and not-yet-garbage-collected sectors
    pub sectors: Cid,
    /// The first block number in this storage provider's current proving period
    pub proving_period_start: u64,
    /// Index of the deadline within the proving period beginning at ProvingPeriodStart that has not yet been finalized
    pub current_deadline: u64,
    /// The sector numbers due for PoSt at each deadline in the current proving period, frozen at period start
    pub deadlines: Cid,
}
```

### Static information about a Storage Provider

The below struct and its fields ensure that all necessary static information about a Storage provider is encapsulated, allowing for efficient management and interaction within the parachain.

```rust
#[derive(Debug, PartialEq, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct StorageProviderInfo<AccountId, PeerId> {
    /// Account that owns this StorageProvider
    /// - Income and returned collateral are paid to this address
    /// 
    /// Rationale: The owner account is essential for economic transactions and permissions management.
    /// By tying the income and collateral to this address, we ensure that the economic benefits and responsibilities
    /// are correctly attributed.
    pub owner: AccountId,

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
