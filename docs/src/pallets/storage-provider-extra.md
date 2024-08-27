<!--
This file contains documentation that didn't make it to the book because it's not implemented yet.

DO NOT INCLUDE IN THE PUBLISHED BOOK!!!
-->

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


## Usage

### Modifying storage provider information

The `Storage Provider Pallet` allows storage providers to modify their information such as changing the peer id, through `change_peer_id` and changing owners, through `change_owner_address`.


## Storage Provider Flow

### Registration

The first thing a storage provider must do is register itself by calling `storage_provider.create_storage_provider(peer_id: PeerId, window_post_proof_type: RegisteredPoStProof)`. At this point there are no funds locked in the storage provider pallet. The next step is to place storage market asks on the market, this is done through the market pallet. After that the storage provider needs to make deals with clients and begin filling up sectors with data. When they have a full sector they should seal the sector.

### Commit

When the storage provider has completed their first seal, they should post it to the storage provider pallet by calling `storage_provider.pre_commit_sector(sectors: SectorPreCommitInfo)`. If the storage provider had zero committed sectors before this call, this begins their proving period. The proving period is a fixed amount of time in which the storage provider must submit a Proof of Space Time to the network.
During this period, the storage provider may also commit to new sectors, but they will not be included in proofs of space time until the next proving period starts. During the prove commit call, the storage provider pledges some collateral in case they fail to submit their PoSt on time.

### Proof of Spacetime submission

When the storage provider has completed their PoSt, they must submit it to the network by calling `storage_provider.submit_windowed_post(deadline: u64, partitions: Vec<u64>, proofs: Vec<PostProof>)`. There are two different types of submissions:

- **Standard Submission**: A standard submission is one that makes it on-chain before the end of the proving period.
- **Penalize Submission**: A penalized submission is one that makes it on-chain after the end of the proving period, but before the generation attack threshold. These submissions count as valid PoSt submissions, but the miner must pay a penalty for their late submission. See [storage fault slashing](#storage-fault-slashing).
