# Storage Provider Pallet

## Overview

The `Storage Provider Pallet` handles the creation of storage providers and facilitates storage providers and client in creating storage deals.

## Usage

### Indexing storage providers

A storage provider indexes in the storage provider pallet itself when it starts up by calling the `create_storage_provider` extrinsic with it's `PeerId` as an argument. The public key will be extracted from the origin and is used to modify on-chain information and receive rewards. The `PeerId` is given by the storage provider so clients can use that to connect to the storage provider.

### Modifying storage provider information

The `Storage Provider Pallet` allows storage providers to modify their information such as changing the peer id, through `change_peer_id` and changing owners, through `change_owner_address`.

## Data structures

```rust
pub struct StorageProviderInfo<
    AccountId: Encode + Decode + Eq + PartialEq,
    PeerId: Encode + Decode + Eq + PartialEq,
> {
    /// The owner of this storage provider.
    owner: AccountId,
    /// Storage provider'ss libp2p peer id in bytes.
    peer_id: PeerId,
}
```

The `StorageProviderInfo` structure holds information about a `StorageProvider`.

```rust
pub type StorageProviders<T: Config> =
    StorageMap<_, _, T::AccountId, StorageProviderInfo<T::AccountId, T::PeerId>>;
```

The `StorageProviders` mapping `AccountId`'s to `PeerId`'s.
