use codec::{Decode, Encode};
use frame_support::{
    ensure,
    pallet_prelude::{ConstU32, RuntimeDebug},
    sp_runtime::BoundedBTreeMap,
};
use scale_info::TypeInfo;
use sp_arithmetic::{traits::BaseArithmetic, ArithmeticError};

use crate::{
    proofs::RegisteredPoStProof,
    sector::{
        SectorNumber, SectorOnChainInfo, SectorPreCommitOnChainInfo, SectorSize, SECTORS_MAX,
    },
};

/// This struct holds the state of a single storage provider.
#[derive(Debug, Decode, Encode, TypeInfo)]
pub struct StorageProviderState<PeerId, Balance, BlockNumber> {
    /// Contains static information about this storage provider
    pub info: StorageProviderInfo<PeerId>,
    /// Information for all proven and not-yet-garbage-collected sectors.
    pub sectors:
        BoundedBTreeMap<SectorNumber, SectorOnChainInfo<BlockNumber>, ConstU32<SECTORS_MAX>>,
    /// Total funds locked as pre_commit_deposit
    /// Optional because when registering there is no need for deposits.
    pub pre_commit_deposits: Balance,
    /// Sectors that have been pre-committed but not yet proven.
    pub pre_committed_sectors: BoundedBTreeMap<
        SectorNumber,
        SectorPreCommitOnChainInfo<Balance, BlockNumber>,
        ConstU32<SECTORS_MAX>,
    >,
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

impl<PeerId, Balance, BlockNumber> StorageProviderState<PeerId, Balance, BlockNumber>
where
    PeerId: Clone + Decode + Encode + TypeInfo,
    BlockNumber: Decode + Encode + TypeInfo,
    Balance: BaseArithmetic,
{
    pub fn new(
        info: &StorageProviderInfo<PeerId>,
        period_start: BlockNumber,
        deadline_idx: BlockNumber,
    ) -> Self {
        Self {
            info: info.clone(),
            sectors: BoundedBTreeMap::new(),
            pre_commit_deposits: 0.into(),
            pre_committed_sectors: BoundedBTreeMap::new(),
            proving_period_start: period_start,
            current_deadline: deadline_idx,
        }
    }

    pub fn add_pre_commit_deposit(&mut self, amount: Balance) -> Result<(), ArithmeticError> {
        self.pre_commit_deposits = self
            .pre_commit_deposits
            .checked_add(&amount)
            .ok_or(ArithmeticError::Overflow)?;
        Ok(())
    }

    // TODO(@aidan46, #107, 2024-06-21): Allow for batch inserts.
    pub fn put_precommitted_sector(
        &mut self,
        precommit: SectorPreCommitOnChainInfo<Balance, BlockNumber>,
    ) -> Result<(), StorageProviderError> {
        let sector_number = precommit.info.sector_number;
        ensure!(
            !self.pre_committed_sectors.contains_key(&sector_number),
            StorageProviderError::SectorAlreadyPreCommitted
        );
        self.pre_committed_sectors
            .try_insert(sector_number, precommit)
            .map_err(|_| StorageProviderError::MaxPreCommittedSectorExceeded)?;

        Ok(())
    }
}

#[derive(RuntimeDebug)]
pub enum StorageProviderError {
    /// Happens when an SP try to commit a sector more than once
    SectorAlreadyPreCommitted,
    /// Happens when an SP tries to pre-commit more sectors than SECTOR_MAX.
    MaxPreCommittedSectorExceeded,
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
