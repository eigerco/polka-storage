use crate::types::{
    RegisteredPoStProof, SectorOnChainInfo, SectorPreCommitOnChainInfo, SectorSize,
};

use codec::{Decode, Encode};
use scale_info::prelude::vec::Vec;
use scale_info::TypeInfo;

/// This struct holds the state of a single storage provider.
#[derive(Debug, Decode, Encode, TypeInfo)]
pub struct StorageProviderState<PeerId, BlockNumber, Balance> {
    /// Contains static information about this storage provider
    pub info: StorageProviderInfo<PeerId>,

    /// Information for all proven and not-yet-garbage-collected sectors.
    pub sectors: Vec<SectorOnChainInfo<BlockNumber>>,

    /// Total funds locked as pre_commit_deposit
    /// Optional because when registering there is no need for deposits.
    pub pre_commit_deposits: Option<Balance>,

    /// Sectors that have been pre-committed but not yet proven.
    pub pre_committed_sectors: Vec<SectorPreCommitOnChainInfo<Balance, BlockNumber>>,
}

impl<PeerId, BlockNumber, Balance> StorageProviderState<PeerId, BlockNumber, Balance>
where
    PeerId: Clone + Decode + Encode + TypeInfo,
    BlockNumber: Decode + Encode + TypeInfo,
{
    pub fn new(info: &StorageProviderInfo<PeerId>) -> Self {
        Self {
            info: info.clone(),
            sectors: Vec::new(),
            pre_commit_deposits: None,
            pre_committed_sectors: Vec::new(),
        }
    }
}

/// Static information about the storage provider.
#[derive(Debug, Clone, Copy, Decode, Encode, TypeInfo, PartialEq)]
pub struct StorageProviderInfo<PeerId> {
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

impl<PeerId> StorageProviderInfo<PeerId> {
    /// Create a new instance of StorageProviderInfo
    pub fn new(peer_id: PeerId, window_post_proof_type: RegisteredPoStProof) -> Self {
        let sector_size = window_post_proof_type.sector_size();

        let window_post_partition_sectors = window_post_proof_type.window_post_partitions_sector();

        Self {
            peer_id,
            window_post_proof_type,
            sector_size,
            window_post_partition_sectors,
        }
    }
}
