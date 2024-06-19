use crate::types::{
    RegisteredPoStProof, SectorOnChainInfo, SectorPreCommitOnChainInfo, SectorSize,
};

use codec::{Decode, Encode};
use primitives::BlockNumber;
use scale_info::prelude::vec::Vec;
use scale_info::TypeInfo;

/// This struct holds the state of a single storage provider.
#[derive(Debug, Decode, Encode, TypeInfo)]
pub struct StorageProviderState<PeerId, Balance> {
    /// Contains static information about this storage provider
    pub info: StorageProviderInfo<PeerId>,

    /// Information for all proven and not-yet-garbage-collected sectors.
    pub sectors: Vec<SectorOnChainInfo>,

    /// Total funds locked as pre_commit_deposit
    /// Optional because when registering there is no need for deposits.
    pub pre_commit_deposits: Option<Balance>,

    /// Sectors that have been pre-committed but not yet proven.
    pub pre_committed_sectors: Vec<SectorPreCommitOnChainInfo<Balance>>,

    /// The first block in this storage provider's current proving period. This is the first block in which a PoSt for a
    /// partition at the storage provider's first deadline may arrive. Alternatively, it is after the last block at which
    /// a PoSt for the previous window is valid.
    /// Always greater than zero, this may be greater than the current block for genesis miners in the first
    /// WPoStProvingPeriod blocks of the chain; the blocks before the first proving period starts are exempt from Window
    /// PoSt requirements.
    /// Updated at the end of every period.
    pub proving_period_start: BlockNumber,

    /// Index of the deadline within the proving period beginning at ProvingPeriodStart that has not yet been
    /// finalized.
    /// Updated at the end of each deadline window.
    pub current_deadline: BlockNumber,
}

impl<PeerId, Balance> StorageProviderState<PeerId, Balance>
where
    PeerId: Clone + Decode + Encode + TypeInfo,
{
    pub fn new(
        info: &StorageProviderInfo<PeerId>,
        period_start: BlockNumber,
        deadline_idx: BlockNumber,
    ) -> Self {
        Self {
            info: info.clone(),
            sectors: Vec::new(),
            pre_commit_deposits: None,
            pre_committed_sectors: Vec::new(),
            proving_period_start: period_start,
            current_deadline: deadline_idx,
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
