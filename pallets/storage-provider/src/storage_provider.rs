extern crate alloc;

use alloc::vec::Vec;

use codec::{Decode, Encode};
use frame_support::{
    pallet_prelude::{ConstU32, RuntimeDebug},
    sp_runtime::{BoundedBTreeMap, BoundedVec},
    PalletError,
};
use primitives_proofs::{RegisteredPoStProof, SectorNumber, SectorSize};
use scale_info::TypeInfo;
use sp_arithmetic::{traits::BaseArithmetic, ArithmeticError};

use crate::{
    deadline::{
        assign_deadlines, deadline_is_mutable, Deadline, DeadlineError, DeadlineInfo, Deadlines,
    },
    sector::{SectorOnChainInfo, SectorPreCommitOnChainInfo, MAX_SECTORS},
};

const LOG_TARGET: &'static str = "runtime::storage_provider::storage_provider";

/// This struct holds the state of a single storage provider.
#[derive(RuntimeDebug, Decode, Encode, TypeInfo)]
pub struct StorageProviderState<PeerId, Balance, BlockNumber>
where
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    /// Contains static information about this storage provider
    pub info: StorageProviderInfo<PeerId>,

    /// Information for all proven and not-yet-garbage-collected sectors.
    pub sectors:
        BoundedBTreeMap<SectorNumber, SectorOnChainInfo<BlockNumber>, ConstU32<MAX_SECTORS>>, // Cannot use ConstU64 here because of BoundedBTreeMap trait bound `Get<u32>`,

    /// Total funds locked as pre_commit_deposit
    pub pre_commit_deposits: Balance,

    /// Sectors that have been pre-committed but not yet proven.
    pub pre_committed_sectors: BoundedBTreeMap<
        SectorNumber,
        SectorPreCommitOnChainInfo<Balance, BlockNumber>,
        ConstU32<MAX_SECTORS>, // Cannot use ConstU64 here because of BoundedBTreeMap trait bound `Get<u32>`
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
    pub current_deadline: u64,

    /// Deadlines indexed by their proving periods — e.g. for proving period 7, find it in
    /// `deadlines[7]` — proving periods are present in the interval `[0, 47]`.
    ///
    /// Bounded to 48 elements since that's the set amount of deadlines per proving period.
    ///
    /// In the original implementation, the information is kept in a separated structure, possibly
    /// to make fetching the state more efficient as this is kept in the storage providers
    /// blockstore. However, we're keeping all the state on-chain
    ///
    /// References:
    /// * <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/miner/src/state.rs#L105-L108>
    /// * <https://spec.filecoin.io/#section-algorithms.pos.post.constants--terminology>
    /// * <https://spec.filecoin.io/#section-algorithms.pos.post.design>
    pub deadlines: Deadlines<BlockNumber>,
}

impl<PeerId, Balance, BlockNumber> StorageProviderState<PeerId, Balance, BlockNumber>
where
    PeerId: Clone + Decode + Encode + TypeInfo,
    BlockNumber: sp_runtime::traits::BlockNumber + BaseArithmetic,
    Balance: BaseArithmetic,
{
    pub fn new(
        info: StorageProviderInfo<PeerId>,
        period_start: BlockNumber,
        deadline_idx: u64,
        w_post_period_deadlines: u64,
    ) -> Self {
        Self {
            info,
            sectors: BoundedBTreeMap::new(),
            pre_commit_deposits: 0.into(),
            pre_committed_sectors: BoundedBTreeMap::new(),
            proving_period_start: period_start,
            current_deadline: deadline_idx,
            deadlines: Deadlines::new(w_post_period_deadlines),
        }
    }

    pub fn add_pre_commit_deposit(&mut self, amount: Balance) -> Result<(), ArithmeticError> {
        self.pre_commit_deposits = self
            .pre_commit_deposits
            .checked_add(&amount)
            .ok_or(ArithmeticError::Overflow)?;
        Ok(())
    }

    /// Inserts sectors into the pre commit state.
    /// Before calling this it should be ensured that the sector number is not being reused.
    // TODO(@aidan46, #107, 2024-06-21): Allow for batch inserts.
    pub fn put_pre_committed_sector(
        &mut self,
        precommit: SectorPreCommitOnChainInfo<Balance, BlockNumber>,
    ) -> Result<(), StorageProviderError> {
        let sector_number = precommit.info.sector_number;
        self.pre_committed_sectors
            .try_insert(sector_number, precommit)
            .map_err(|_| StorageProviderError::MaxPreCommittedSectorExceeded)?;

        Ok(())
    }

    /// Get a pre committed sector from the given sector number.
    pub fn get_pre_committed_sector(
        &self,
        sector_number: SectorNumber,
    ) -> Result<&SectorPreCommitOnChainInfo<Balance, BlockNumber>, StorageProviderError> {
        self.pre_committed_sectors
            .get(&sector_number)
            .ok_or(StorageProviderError::SectorNotFound)
    }

    /// Removes a pre committed sector from the given sector number.
    pub fn remove_pre_committed_sector(
        &mut self,
        sector_num: SectorNumber,
    ) -> Result<(), StorageProviderError> {
        self.pre_committed_sectors
            .remove(&sector_num)
            .ok_or(StorageProviderError::SectorNotFound)?;
        Ok(())
    }

    /// Activates a given sector according to the sector number
    ///
    /// Before this call the sector number should be checked for collisions.
    pub fn activate_sector(
        &mut self,
        sector_num: SectorNumber,
        info: SectorOnChainInfo<BlockNumber>,
    ) -> Result<(), StorageProviderError> {
        self.sectors
            .try_insert(sector_num, info)
            .map_err(|_| StorageProviderError::SectorNumberInUse)?;
        Ok(())
    }

    /// Assign new sector to a deadline.
    ///
    /// Reference:
    /// * <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/miner/src/state.rs#L489-L554>
    pub fn assign_sectors_to_deadlines(
        &mut self,
        current_block: BlockNumber,
        mut sectors: BoundedVec<SectorOnChainInfo<BlockNumber>, ConstU32<MAX_SECTORS>>,
        partition_size: u64,
        max_partitions_per_deadline: u64,
        fault_cutoff_declaration: BlockNumber,
        w_post_period_deadlines: u64,
        w_post_proving_period: BlockNumber,
        w_post_challenge_window: BlockNumber,
        w_post_challenge_lookback: BlockNumber,
    ) -> Result<(), StorageProviderError> {
        sectors.sort_by_key(|info| info.sector_number);

        log::debug!(target: LOG_TARGET,
            "assign_sectors_to_deadlines: deadline len = {}",
            self.deadlines.len()
        );

        let mut deadline_vec: Vec<Option<Deadline<BlockNumber>>> =
            (0..w_post_period_deadlines).map(|_| None).collect();

        // required otherwise the logic gets complicated really fast
        // the issue is that filecoin supports negative epoch numbers
        if current_block < self.proving_period_start {
            // Before the firs
            for (idx, deadline) in self.deadlines.due.iter().enumerate() {
                deadline_vec[idx as usize] = Some(deadline.clone());
            }
        } else {
            for (idx, deadline) in self.deadlines.due.iter().enumerate() {
                let is_deadline_mutable = deadline_is_mutable(
                    self.proving_period_start,
                    idx as u64,
                    current_block,
                    fault_cutoff_declaration,
                    w_post_period_deadlines,
                    w_post_proving_period,
                    w_post_challenge_window,
                    w_post_challenge_lookback,
                )?;
                log::error!(target: LOG_TARGET, "is_deadline_mutable {}", is_deadline_mutable);
                // Skip deadlines that aren't currently mutable.
                if is_deadline_mutable {
                    deadline_vec[idx as usize] = Some(deadline.clone());
                }
            }
        }

        let deadline_to_sectors = assign_deadlines(
            max_partitions_per_deadline,
            partition_size,
            &deadline_vec,
            &sectors,
            w_post_period_deadlines,
        )?;
        for (deadline_idx, deadline_sectors) in deadline_to_sectors.enumerate() {
            if deadline_sectors.is_empty() {
                continue;
            }

            let deadline =
                deadline_vec[deadline_idx]
                    .as_mut()
                    .ok_or(StorageProviderError::DeadlineError(
                        DeadlineError::CouldNotAssignSectorsToDeadlines,
                    ))?;

            deadline.add_sectors(partition_size, &deadline_sectors)?;
            self.deadlines.due[deadline_idx] = deadline.clone();
        }
        Ok(())
    }

    /// Simple getter for mutable deadlines.
    pub fn get_deadlines_mut(&mut self) -> &mut Deadlines<BlockNumber> {
        &mut self.deadlines
    }

    /// Returns deadline calculations for the current (according to state) proving period.
    ///
    /// **Pre-condition**: `current_block > self.proving_period_start`
    pub fn deadline_info(
        &self,
        current_block: BlockNumber,
        fault_cutoff_declaration: BlockNumber,
        w_post_period_deadlines: u64,
        w_post_proving_period: BlockNumber,
        w_post_challenge_window: BlockNumber,
        w_post_challenge_lookback: BlockNumber,
    ) -> Result<DeadlineInfo<BlockNumber>, DeadlineError> {
        let current_deadline_index = calculate_current_deadline_index(
            current_block,
            self.proving_period_start,
            w_post_challenge_window,
        );

        // convert to u64
        let current_deadline_index: u64 = current_deadline_index
            .try_into()
            .map_err(|_| DeadlineError::CouldNotConstructDeadlineInfo)?;

        DeadlineInfo::new(
            current_block,
            self.proving_period_start,
            current_deadline_index,
            fault_cutoff_declaration,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
        )
    }
}

/// Errors that can occur while interacting with the storage provider state.
#[derive(Decode, Encode, PalletError, TypeInfo, RuntimeDebug)]
pub enum StorageProviderError {
    /// Happens when an SP tries to pre-commit more sectors than SECTOR_MAX.
    MaxPreCommittedSectorExceeded,
    /// Happens when trying to access a sector that does not exist.
    SectorNotFound,
    /// Happens when a sector number is already in use.
    SectorNumberInUse,
    /// Wrapper around [`DeadlineError`]
    DeadlineError(crate::deadline::DeadlineError),
}

impl From<DeadlineError> for StorageProviderError {
    fn from(dl_err: DeadlineError) -> Self {
        Self::DeadlineError(dl_err)
    }
}

/// Static information about the storage provider.
#[derive(RuntimeDebug, Clone, Copy, Decode, Encode, TypeInfo, PartialEq)]
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

/// Calculate the *first* proving period.
///
/// *This function deviates considerably from Filecoin.*
///
/// Since our block number (equivalent to `ChainEpoch`) is unsigned, we are not afforded the
/// luxury of calculating "current proving period" as it generates edge cases for the first
/// storage providers being registered, that is, before [`Config::WPoStChallengeWindow`] blocks
/// have elapsed).
///
/// This method will calculate the current global proving period start and add the offset to it.
/// You can read how to calculate the global proving period start and index in the description
/// for [`Config::WPoStProvingWindow`].
///
/// Reference:
/// * <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/miner/src/lib.rs#L4904-L4921>
pub(crate) fn calculate_first_proving_period_start<BlockNumber>(
    current_block: BlockNumber,
    offset: BlockNumber,
    wpost_proving_period: BlockNumber,
) -> BlockNumber
where
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    let global_proving_index = current_block / wpost_proving_period;
    // +1 to get the next proving period, ensuring the start is always in the future and
    // and the absolute first time the SP needs to start submitting proofs
    let global_proving_start = (global_proving_index + BlockNumber::one()) * wpost_proving_period;

    global_proving_start + offset
}

/// Calculate the current deadline index.
///
/// **Pre-condition**: `current_block >= period_start`
///
/// No magic here, the same logic from Filecoin applies.
///
/// Reference:
/// * <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/miner/src/lib.rs#L4923-L4929>
pub(crate) fn calculate_current_deadline_index<BlockNumber>(
    current_block: BlockNumber,
    period_start: BlockNumber,
    w_post_challenge_period: BlockNumber,
) -> BlockNumber
where
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    (current_block - period_start) / w_post_challenge_period
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::storage_provider::calculate_first_proving_period_start;

    // Adding +120 since it's always one full proving period ahead
    #[rstest]
    #[case(0, 0, 120)]
    #[case(0, 119, 120 + 119)]
    #[case(1, 0, 120)]
    #[case(1, 119, 120 + 119)]
    #[case(120, 0, 120 + 120)]
    #[case(120, 20, 120 + 140)]
    #[case(124, 0, 120 + 120)]
    #[case(124, 20, 120 + 140)]
    #[case(20, 5, 120 + 5)]
    fn calculate_proving_period(
        #[case] current_block: u64,
        #[case] offset: u64,
        #[case] expected_start: u64,
    ) {
        assert_eq!(
            calculate_first_proving_period_start::<u64>(current_block, offset, 120),
            expected_start
        );
    }
}
