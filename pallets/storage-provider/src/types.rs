use codec::{Decode, Encode};
use scale_info::prelude::string::String;
use scale_info::prelude::vec::Vec;
use scale_info::TypeInfo;

/// SectorNumber is a numeric identifier for a sector.
pub type SectorNumber = u64;

/// Content identifier
pub type Cid = String;

#[derive(Decode, Encode, TypeInfo)]
pub struct StorageProviderInfo<
    AccountId: Encode + Decode + Eq + PartialEq,
    PeerId: Encode + Decode + Eq + PartialEq,
> {
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

impl<PeerId, AccountId> StorageProviderInfo<AccountId, PeerId>
where
    AccountId: Encode + Decode + Eq + PartialEq,
    PeerId: Encode + Decode + Eq + PartialEq + Clone,
{
    /// Create a new instance of StorageProviderInfo
    pub fn new(
        owner: AccountId,
        peer_id: PeerId,
        window_post_proof_type: RegisteredPoStProof,
    ) -> Result<Self, String> {
        let sector_size = window_post_proof_type.sector_size();

        let window_post_partition_sectors = window_post_proof_type.window_post_partitions_sector();

        Ok(Self {
            owner,
            peer_id,
            window_post_proof_type,
            sector_size,
            window_post_partition_sectors,
        })
    }

    /// Updates the owner address.
    pub fn change_owner(&self, owner: AccountId) -> Self {
        Self {
            owner,
            peer_id: self.peer_id.clone(),
            window_post_proof_type: self.window_post_proof_type,
            sector_size: self.sector_size,
            window_post_partition_sectors: self.window_post_partition_sectors,
        }
    }
}

/// SectorSize indicates one of a set of possible sizes in the network.
#[derive(Encode, Decode, TypeInfo, Clone, Debug, PartialEq, Eq, Copy)]
pub enum SectorSize {
    2KiB,
}

/// Proof of Spacetime type, indicating version and sector size of the proof.
#[derive(Debug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone, Copy)]
pub enum RegisteredPoStProof {
    StackedDRGWindow2KiBV1P1,
}

impl RegisteredPoStProof {
    /// Returns the sector size of the proof type, which is measured in bytes.
    pub fn sector_size(self) -> SectorSize {
        use RegisteredPoStProof::*;
        match self {
            StackedDRGWindow2KiBV1P1 => SectorSize::2KiB,
        }
    }

    /// Proof size for each PoStProof type
    #[allow(unused)]
    pub fn proof_size(self) -> usize {
        use RegisteredPoStProof::*;
        match self {
            StackedDRGWindow2KiBV1P1 => 192,
        }
    }
    /// Returns the partition size, in sectors, associated with a proof type.
    /// The partition size is the number of sectors proven in a single PoSt proof.
    pub fn window_post_partitions_sector(self) -> u64 {
        // Resolve to post proof and then compute size from that.
        use RegisteredPoStProof::*;
        match self {
            StackedDRGWindow2KiBV1P1 => 2,
        }
    }
}

/// Proof of Spacetime data stored on chain.
#[derive(Debug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone)]
pub struct PoStProof {
    pub post_proof: RegisteredPoStProof,
    pub proof_bytes: Vec<u8>,
}

/// Seal proof type which defines the version and sector size.
#[allow(non_camel_case_types)]
#[derive(Debug, Decode, Encode, TypeInfo, Eq, PartialEq, Clone)]
pub enum RegisteredSealProof {
    StackedDRG2KiBV1P1,
}

/// This type is passed into the pre commit function on the storage provider pallet
#[derive(Debug, Decode, Encode, TypeInfo, Eq, PartialEq, Clone)]
pub struct SectorPreCommitInfo {
    pub seal_proof: RegisteredSealProof,
    pub sector_number: SectorNumber,
    pub sealed_cid: Cid,
    pub expiration: u64,
}
