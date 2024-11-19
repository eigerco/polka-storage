extern crate alloc;

use alloc::{collections::BTreeSet, vec::Vec};

use codec::{Decode, Encode};
use frame_support::{
    pallet_prelude::{ConstU32, RuntimeDebug},
    sp_runtime::{BoundedBTreeMap, BoundedVec},
};
use primitives_proofs::{RegisteredPoStProof, SectorNumber, SectorSize};
use scale_info::TypeInfo;
use sp_arithmetic::{traits::BaseArithmetic, ArithmeticError};

use crate::{
    deadline::{assign_deadlines, deadline_is_mutable, Deadline, DeadlineInfo, Deadlines},
    error::GeneralPalletError,
    partition::TerminationResult,
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

    /// Deadlines with outstanding fees for early sector termination.
    pub early_terminations: BTreeSet<u64>,
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
            early_terminations: BTreeSet::new(),
        }
    }

    /// Advance the proving period start of the storage provider if the next deadline is the first one.
    pub fn advance_deadline(
        &mut self,
        current_block: BlockNumber,
        w_post_period_deadlines: u64,
        w_post_proving_period: BlockNumber,
        w_post_challenge_window: BlockNumber,
        w_post_challenge_lookback: BlockNumber,
        fault_declaration_cutoff: BlockNumber,
    ) -> Result<(), GeneralPalletError> {
        let dl_info = self.deadline_info(
            current_block,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_declaration_cutoff,
        )?;

        if !dl_info.period_started() {
            return Ok(());
        }

        self.current_deadline = (self.current_deadline + 1) % w_post_period_deadlines;
        log::debug!(target: LOG_TARGET, "new deadline {:?}, period deadlines {:?}",
        self.current_deadline, w_post_period_deadlines);

        if self.current_deadline == 0 {
            self.proving_period_start = self.proving_period_start + w_post_proving_period;
        }

        let deadline = self.deadlines.load_deadline_mut(dl_info.idx as usize)?;

        // Expire sectors that are due, either for on-time expiration or "early" faulty-for-too-long.
        let expired = deadline.pop_expired_sectors(dl_info.last())?;
        let early_terminations = !expired.early_sectors.is_empty();
        if early_terminations {
            self.early_terminations.insert(dl_info.idx);
        }
        Ok(())
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
    ) -> Result<(), GeneralPalletError> {
        let sector_number = precommit.info.sector_number;
        self.pre_committed_sectors
            .try_insert(sector_number, precommit)
            .map_err(|_| {
                log::error!(target: LOG_TARGET, "put_pre_committed_sector: Failed to insert pre committed sector {sector_number:?}");
                GeneralPalletError::StorageProviderErrorMaxPreCommittedSectorExceeded
            })?;

        Ok(())
    }

    /// Get a pre committed sector from the given sector number.
    pub fn get_pre_committed_sector(
        &self,
        sector_number: SectorNumber,
    ) -> Result<&SectorPreCommitOnChainInfo<Balance, BlockNumber>, GeneralPalletError> {
        self.pre_committed_sectors
            .get(&sector_number)
            .ok_or_else(|| {
                log::error!(target: LOG_TARGET, "get_pre_committed_sector: Failed to get pre committed sector {sector_number:?}");
                GeneralPalletError::StorageProviderErrorSectorNotFound
            })
    }

    /// Removes a pre committed sector from the given sector number.
    pub fn remove_pre_committed_sector(
        &mut self,
        sector_num: SectorNumber,
    ) -> Result<(), GeneralPalletError> {
        if self.pre_committed_sectors.remove(&sector_num).is_none() {
            log::error!(target: LOG_TARGET, "remove_pre_committed_sector: Failed to remove pre committed sector {sector_num:?}");
            return Err(GeneralPalletError::StorageProviderErrorSectorNotFound);
        }
        Ok(())
    }

    /// Activates a given sector according to the sector number
    ///
    /// Before this call the sector number should be checked for collisions.
    pub fn activate_sector(
        &mut self,
        sector_num: SectorNumber,
        info: SectorOnChainInfo<BlockNumber>,
    ) -> Result<(), GeneralPalletError> {
        self.sectors
            .try_insert(sector_num, info)
            .map_err(|_| {
                log::error!(target: LOG_TARGET, "activate_sector: Failed to activate {sector_num:?} because that sector number is in use");
                GeneralPalletError::StorageProviderErrorSectorNumberInUse
            })?;
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
        w_post_period_deadlines: u64,
        w_post_proving_period: BlockNumber,
        w_post_challenge_window: BlockNumber,
        w_post_challenge_lookback: BlockNumber,
        fault_declaration_cutoff: BlockNumber,
    ) -> Result<(), GeneralPalletError> {
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
                    w_post_period_deadlines,
                    w_post_proving_period,
                    w_post_challenge_window,
                    w_post_challenge_lookback,
                    fault_declaration_cutoff,
                )?;
                if is_deadline_mutable {
                    log::debug!(target: LOG_TARGET, "deadline[{idx}] is mutable");
                } else {
                    log::debug!(target: LOG_TARGET, "deadline[{idx}] is not mutable");
                }
                // Skip deadlines that aren't currently mutable.
                if is_deadline_mutable {
                    deadline_vec[idx as usize] = Some(deadline.clone());
                }
            }
        }

        // Assign sectors to deadlines.
        let deadline_to_sectors = assign_deadlines(
            max_partitions_per_deadline,
            partition_size,
            &deadline_vec,
            &sectors,
            w_post_period_deadlines,
        )?;

        for (deadline_idx, deadline_sectors) in deadline_to_sectors.iter().enumerate() {
            if deadline_sectors.is_empty() {
                continue;
            }

            let deadline = deadline_vec[deadline_idx]
                .as_mut()
                .ok_or(GeneralPalletError::DeadlineErrorCouldNotAssignSectorsToDeadlines)?;

            deadline.add_sectors(partition_size, deadline_sectors)?;
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
        w_post_period_deadlines: u64,
        w_post_proving_period: BlockNumber,
        w_post_challenge_window: BlockNumber,
        w_post_challenge_lookback: BlockNumber,
        fault_declaration_cutoff: BlockNumber,
    ) -> Result<DeadlineInfo<BlockNumber>, GeneralPalletError> {
        let current_deadline_index = self.current_deadline;

        DeadlineInfo::new(
            current_block,
            self.proving_period_start,
            current_deadline_index,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_declaration_cutoff,
        )
    }

    /// Pops up to `max_sectors` early terminated sectors from all deadlines.
    ///
    /// Returns `true` if we still have more early terminations to process.
    pub fn pop_early_terminations(
        &mut self,
        max_partitions: u64,
        max_sectors: u64,
    ) -> Result<TerminationResult<BlockNumber>, GeneralPalletError> {
        // Anything to do? This lets us avoid loading the deadlines if there's nothing to do.
        if self.early_terminations.is_empty() {
            log::info!("early terminations empty");
            return Ok(TerminationResult::new());
        }

        let mut result = TerminationResult::new();
        let mut to_unset = Vec::new();

        for &dl_idx in self.early_terminations.iter() {
            let deadline = self.deadlines.load_deadline_mut(dl_idx as usize)?;

            let (deadline_result, more) = deadline.pop_early_terminations(
                max_partitions - result.partitions_processed,
                max_sectors - result.sectors_processed,
            )?;

            result += deadline_result;

            if !more {
                to_unset.push(dl_idx);
            }

            if !result.below_limit(max_partitions, max_sectors) {
                break;
            }
        }

        for deadline_idx in to_unset {
            self.early_terminations.remove(&deadline_idx);
        }

        Ok(result)
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
