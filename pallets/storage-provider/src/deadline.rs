extern crate alloc;

use alloc::vec::Vec;

use codec::{Decode, Encode};
use frame_support::{pallet_prelude::*, sp_runtime::BoundedBTreeMap};
use primitives_proofs::SectorNumber;
use scale_info::{prelude::cmp, TypeInfo};

use crate::{
    error::GeneralPalletError,
    partition::{Partition, PartitionNumber, TerminationResult, MAX_PARTITIONS_PER_DEADLINE},
    sector::{SectorOnChainInfo, MAX_SECTORS},
    sector_map::PartitionMap,
};

mod assignment;

pub use assignment::assign_deadlines;

const LOG_TARGET: &'static str = "runtime::storage_provider::deadline";

/// Deadline holds the state for all sectors due at a specific deadline.
///
/// A deadline exists along side 47 other deadlines (1 for every 30 minutes in a day).
/// Only one deadline may be active for a given proving window.
#[derive(Clone, RuntimeDebug, Default, Decode, Encode, PartialEq, TypeInfo)]
pub struct Deadline<BlockNumber>
where
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    /// Partitions in this deadline. Indexed by partition number.
    pub partitions: BoundedBTreeMap<
        PartitionNumber,
        Partition<BlockNumber>,
        ConstU32<MAX_PARTITIONS_PER_DEADLINE>,
    >,

    /// Maps blocks to partitions that _may_ have sectors about to expire — i.e. just before or in that block.
    /// The expiration happens either on-time or early because faults.
    ///
    /// Filecoin has another expiration mapping in the Partition struct which maps the a block to sectors that are on-time or expired (due to being faulty).
    /// We can extract this information from other sources.
    /// The faulty sectors are stored in the Partition and the sectors that are on-time are sectors - (faults + terminated + unproven + recoveries).
    ///
    /// Getting the information about a partition that has sectors that are about to expire you need to get the current deadline from the storage provider state.
    /// `let current_deadline_block = storage_provider_state.current_deadline;`
    /// With the current deadline we can then get the partition number that is associated with that deadline block.
    /// `let partition_number = deadline.expirations_blocks.get(current_deadline_block);`
    ///
    /// Then we can get the partition information from the deadline.
    /// `let partition_to_expire = deadline.partitions.get(partition_number);`
    ///
    /// With this information we can get the sectors information from the storage provider state.
    /// `let sectors_info = partition_to_expire.`
    /// Then we can get the sector information.
    /// `let sectors_info: Vec<SectorOnChainInfo<BlockNumber> = partition_to_expire.sectors.iter().map(|sector_number| {
    ///     storage_provider_state.sectors.get(sector_number)
    /// }).collect()`
    ///
    /// # Important
    /// Partitions MUST NOT be removed from this queue (until the
    /// associated block has passed) even if they no longer have sectors
    /// expiring at that block. Sectors expiring at their given block may later be
    /// recovered, and this queue will not be updated at that time.
    pub expirations_blocks:
        BoundedBTreeMap<BlockNumber, PartitionNumber, ConstU32<MAX_PARTITIONS_PER_DEADLINE>>,

    /// Partitions that have been proved by window PoSts so far during the
    /// current challenge window.
    pub partitions_posted: BoundedBTreeSet<PartitionNumber, ConstU32<MAX_PARTITIONS_PER_DEADLINE>>,

    /// Partition numbers with sectors that terminated early.
    pub early_terminations: BoundedBTreeSet<PartitionNumber, ConstU32<MAX_PARTITIONS_PER_DEADLINE>>,

    /// The number of non-terminated sectors in this deadline (incl faulty).
    pub live_sectors: u64,

    /// The total number of sectors in this deadline (incl dead).
    pub total_sectors: u64,
}

impl<BlockNumber> Deadline<BlockNumber>
where
    BlockNumber: sp_runtime::traits::BlockNumber + Copy,
{
    /// Construct a new [`Deadline`] instance.
    pub fn new() -> Self {
        Self {
            partitions: BoundedBTreeMap::new(),
            expirations_blocks: BoundedBTreeMap::new(),
            partitions_posted: BoundedBTreeSet::new(),
            early_terminations: BoundedBTreeSet::new(),
            live_sectors: 0,
            total_sectors: 0,
        }
    }

    /// Sets a given partition as proven.
    /// Proving also recovers a partition (unmarks it as faulty),
    /// if the partition was declared to recover before submitting a proof.
    ///
    /// If the partition has already been proven, an error is returned.
    pub fn record_proven(
        &mut self,
        all_sectors: &BoundedBTreeMap<
            SectorNumber,
            SectorOnChainInfo<BlockNumber>,
            ConstU32<MAX_SECTORS>,
        >,
        partitions: BoundedVec<PartitionNumber, ConstU32<MAX_PARTITIONS_PER_DEADLINE>>,
    ) -> Result<(), GeneralPalletError> {
        for partition_num in partitions {
            log::debug!(target: LOG_TARGET, "record_proven: partition number = {partition_num:?}");

            let partition = self.partitions.get_mut(&partition_num).ok_or_else(|| {
                log::error!(target: LOG_TARGET, "record_proven: partition {partition_num:?} not found");
                GeneralPalletError::DeadlineErrorPartitionNotFound
            })?;

            // Ensure the partition hasn't already been proven.
            ensure!(!self.partitions_posted.contains(&partition_num), {
                log::error!(target: LOG_TARGET, "record_proven: partition {partition_num:?} already proven");
                GeneralPalletError::DeadlineErrorPartitionAlreadyProven
            });

            // Record the partition as proven.
            self.partitions_posted
                .try_insert(partition_num)
                .map_err(|_| {
                    log::error!(target: LOG_TARGET, "record_proven: Error while trying to insert partitions");
                    GeneralPalletError::DeadlineErrorProofUpdateFailed
                })?;

            partition.recover_all_declared_recoveries(all_sectors).map_err(|e| {
                log::error!(target: LOG_TARGET, e:?; "record_proven: failed to recover all declared recoveries for partition {partition_num:?}");
                e
            })?;
        }

        Ok(())
    }

    /// Adds sectors to the current deadline.
    ///
    /// Added sectors will be stored in the deadline's last stored partition.
    ///
    /// # Important
    /// * It's the caller's responsibility to make sure that this deadline isn't currently being proven — i.e. open.
    /// * The sectors are assumed to be non-faulty.
    pub fn add_sectors(
        &mut self,
        partition_size: u64,
        sectors: &[SectorOnChainInfo<BlockNumber>],
    ) -> Result<(), GeneralPalletError> {
        if sectors.is_empty() {
            return Ok(());
        }

        // First update partitions, consuming the sectors
        let mut partition_deadline_updates =
            Vec::<(BlockNumber, PartitionNumber)>::with_capacity(sectors.len());
        // PRE-COND: there can never be more live sectors than u64, so it never overflows
        self.live_sectors += sectors.len() as u64;
        self.total_sectors += sectors.len() as u64;

        // Take ownership of underlying map and convert it into inner BTree to be able to use `.entry` API.
        let mut partitions = core::mem::take(&mut self.partitions).into_inner();
        let initial_partitions = partitions.len();

        // We can always start at the last partition. That is because we know
        // that partitions before the last one are full. We achieve that by
        // filling a new partition only when the current one is full.
        let mut partition_idx = initial_partitions.saturating_sub(1);
        loop {
            // Get the partition to which we want to add sectors. If the
            // partition does not exist, create a new one. The new partition is
            // created when it's our first time adding sectors to it.
            let partition = partitions
                .entry(partition_idx as u32)
                .or_insert(Partition::new());

            // Get the current partition's sector count. If the current
            // partition is full, create a new one and start filling that one.
            let sector_count = partition.sectors.len() as u64;
            if sector_count >= partition_size {
                partition_idx += 1;
                continue;
            }

            // Calculate how many sectors we can add to current partition.
            let size = cmp::min(partition_size - sector_count, sectors.len() as u64) as usize;

            // Split the sectors into two parts: one to add to the current
            // partition and the rest which will be added to the next one.
            let (partition_new_sectors, sectors) = sectors.split_at(size);

            // Add new sector numbers to the current partition.
            partition.add_sectors(&partition_new_sectors)?;

            // Record deadline -> partition mapping so we can later update the deadlines.
            partition_deadline_updates.extend(
                partition_new_sectors
                    .iter()
                    .map(|s| (s.expiration, partition_idx as PartitionNumber)),
            );

            // No more sectors to add
            if sectors.is_empty() {
                break;
            }
        }

        let partitions = BoundedBTreeMap::try_from(partitions).map_err(|_| {
            log::error!(target: LOG_TARGET, "add_sectors: could not convert partitions to BoundedBTreeMap, too many of them ({} -> {}).",
                initial_partitions,
                partition_idx);
            GeneralPalletError::DeadlineErrorCouldNotAddSectors
        })?;
        // Ignore the default value placed by `take`
        let _ = core::mem::replace(&mut self.partitions, partitions);

        // Next, update the expiration queue.
        for (block, partition_index) in partition_deadline_updates {
            self.expirations_blocks.try_insert(block, partition_index).map_err(|_| {
                log::error!(target: LOG_TARGET, "add_sectors: Cannot update expiration queue at index {partition_idx}");
                GeneralPalletError::DeadlineErrorCouldNotAddSectors
            })?;
        }

        Ok(())
    }

    /// Records the partitions passed in as faulty.
    /// Filecoin ref: <https://github.com/filecoin-project/builtin-actors/blob/82d02e58f9ef456aeaf2a6c737562ac97b22b244/actors/miner/src/deadline_state.rs#L759>
    pub fn record_faults(
        &mut self,
        sectors: &BoundedBTreeMap<
            SectorNumber,
            SectorOnChainInfo<BlockNumber>,
            ConstU32<MAX_SECTORS>,
        >,
        partition_sectors: &mut PartitionMap,
        fault_expiration_block: BlockNumber,
    ) -> Result<(), GeneralPalletError> {
        for (partition_number, faulty_sectors) in partition_sectors.0.iter() {
            let partition = self
                .partitions
                .get_mut(partition_number)
                .ok_or(GeneralPalletError::DeadlineErrorPartitionNotFound)?;

            // Whether all sectors that we declare as faulty actually exist
            ensure!(faulty_sectors.iter().all(|s| sectors.contains_key(&s)), {
                log::error!(target: LOG_TARGET, "record_faults: sectors {:?} not found in the storage provider", faulty_sectors);
                GeneralPalletError::DeadlineErrorSectorsNotFound
            });

            partition.record_faults(
                sectors,
                faulty_sectors,
                fault_expiration_block
            ).map_err(|e| {
                log::error!(target: LOG_TARGET, e:?; "record_faults: Error while recording faults in a partition");
                e
            })?;

            // Update expiration block
            if let Some((&block, _)) = self
                .expirations_blocks
                .iter()
                .find(|(_, partition_num)| partition_num == &partition_number)
            {
                self.expirations_blocks.remove(&block);
                self.expirations_blocks.try_insert(fault_expiration_block, *partition_number).map_err(|_| {
                    log::error!(target: LOG_TARGET, "record_faults: Could not insert new expiration");
                    GeneralPalletError::DeadlineErrorFailedToUpdateFaultExpiration
                })?;
            } else {
                self.expirations_blocks.try_insert(fault_expiration_block, *partition_number).map_err(|_| {
                    log::error!(target: LOG_TARGET, "record_faults: Could not insert new expiration");
                    GeneralPalletError::DeadlineErrorFailedToUpdateFaultExpiration
                })?;
            }
        }

        Ok(())
    }

    /// Sets sectors as recovering.
    /// Filecoin ref: <https://github.com/filecoin-project/builtin-actors/blob/0f205c378983ac6a08469b9f400cbb908eef64e2/actors/miner/src/deadline_state.rs#L818>
    pub fn declare_faults_recovered(
        &mut self,
        sectors: &BoundedBTreeMap<
            SectorNumber,
            SectorOnChainInfo<BlockNumber>,
            ConstU32<MAX_SECTORS>,
        >,
        partition_sectors: &PartitionMap,
    ) -> Result<(), GeneralPalletError> {
        for (partition_number, recovered_sectors) in partition_sectors.0.iter() {
            let partition = self
                .partitions
                .get_mut(partition_number)
                .ok_or_else(|| {
                    log::error!(target: LOG_TARGET, "declare_faults_recovered: Could not find partition {partition_number}");
                    GeneralPalletError::DeadlineErrorPartitionNotFound
                })?;

            // Whether all sectors that we declare as recovered actually exist
            ensure!(
                recovered_sectors.iter().all(|s| sectors.contains_key(&s)),
                {
                    log::error!(target: LOG_TARGET, "record_faults: sectors {:?} not found in the storage provider", recovered_sectors);
                    GeneralPalletError::DeadlineErrorSectorsNotFound
                }
            );

            // Whether all sectors that we declare as recovered were faulty previously
            ensure!(
                recovered_sectors
                    .iter()
                    .all(|s| partition.faults.contains(&s)),
                {
                    log::error!(target: LOG_TARGET, "record_faults: sectors {:?} were not all marked as faulty before", recovered_sectors);
                    GeneralPalletError::DeadlineErrorSectorsNotFaulty
                }
            );

            partition.declare_faults_recovered(recovered_sectors);
        }

        Ok(())
    }

    /// Terminates sectors in the given partitions at the given block number
    /// Fails if any of the partitions given in the `partition_numbers` is not found.
    /// This functions is invoked by the `terminate_sectors` extrinsic when an SP calls that extrinsic
    ///
    /// Reference implementation:
    /// * <https://github.com/filecoin-project/builtin-actors/blob/8d957d2901c0f2044417c268f0511324f591cb92/actors/miner/src/deadline_state.rs#L568>
    pub fn terminate_sectors(
        &mut self,
        block_number: BlockNumber,
        sectors: &[SectorOnChainInfo<BlockNumber>],
        partition_numbers: &[PartitionNumber],
    ) -> Result<(), GeneralPalletError> {
        for &partition_number in partition_numbers {
            let partition = self
                .partitions
                .get_mut(&partition_number).ok_or({
                    log::error!(target: LOG_TARGET, "terminate_sectors: Cannot find partition {partition_number}");
                    GeneralPalletError::DeadlineErrorPartitionNotFound
                })?;

            let removed = partition.terminate_sectors(block_number, sectors)?;

            if !removed.is_empty() {
                // Record that partition now has pending early terminations.
                self.early_terminations
                    .try_insert(partition_number)
                    .expect("Cannot have more terminations than MAX_PARTITIONS_PER_DEADLINE");

                // Record change to sectors
                self.live_sectors -= removed.len() as u64;
            }
        }
        Ok(())
    }

    /// Pops early terminations until `max_sectors`, `max_partitions` or until there are none left
    ///
    /// Reference implementation:
    /// * <https://github.com/filecoin-project/builtin-actors/blob/8d957d2901c0f2044417c268f0511324f591cb92/actors/miner/src/deadline_state.rs#L489>
    pub fn pop_early_terminations(
        &mut self,
        max_partitions: u64,
        max_sectors: u64,
    ) -> Result<(TerminationResult<BlockNumber>, /* has more */ bool), GeneralPalletError> {
        let mut partitions_finished = Vec::new();
        let mut result = TerminationResult::new();

        for &partition_number in self.early_terminations.iter() {
            let mut partition = match self.partitions.get_mut(&partition_number) {
                Some(partition) => partition.clone(),
                None => {
                    partitions_finished.push(partition_number);
                    continue;
                }
            };

            // Pop early terminations
            let (partition_result, more) =
                partition.pop_early_terminations(max_sectors - result.sectors_processed)?;

            result += partition_result;

            // If we've processed all of them for this partition, unmark it in the deadline.
            if !more {
                partitions_finished.push(partition_number);
            }

            // Save partition
            self.partitions
                .try_insert(partition_number, partition)
                .expect("Could not replace existing partition");

            if !result.below_limit(max_partitions, max_sectors) {
                break;
            }
        }

        // Removed finished partitions
        for finished in partitions_finished {
            self.early_terminations.remove(&finished);
        }

        let no_early_terminations = self.early_terminations.iter().next().is_none();

        Ok((result, !no_early_terminations))
    }
}

#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq, TypeInfo)]
pub struct Deadlines<BlockNumber>
where
    BlockNumber: sp_runtime::traits::BlockNumber,
{
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
    pub due: BoundedVec<Deadline<BlockNumber>, ConstU32<48>>,
}

impl<BlockNumber> Deadlines<BlockNumber>
where
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    /// Construct a new [`Deadlines`].
    ///
    /// Pre-initializes all the `w_post_period_deadlines` as empty deadlines.
    pub fn new(w_post_period_deadlines: u64) -> Self {
        let mut due = BoundedVec::new();
        for _ in 0..w_post_period_deadlines {
            let _ = due.try_push(Deadline::new());
        }
        Self { due }
    }

    /// Get the amount of deadlines that are due.
    pub fn len(&self) -> usize {
        self.due.len()
    }

    /// Loads a mutable deadline from the given index.
    /// Fails if the index does not exist or is out of range.
    pub fn load_deadline_mut(
        &mut self,
        idx: usize,
    ) -> Result<&mut Deadline<BlockNumber>, GeneralPalletError> {
        log::debug!(target: LOG_TARGET, "load_deadline_mut: getting deadline at index {idx}");
        // Ensure the provided index is within range.
        ensure!(
            self.len() > idx,
            GeneralPalletError::DeadlineErrorDeadlineIndexOutOfRange
        );
        if let Some(deadline) = self.due.get_mut(idx) {
            Ok(deadline)
        } else {
            log::error!(target: LOG_TARGET, "load_deadline_mut: Failed to get deadline at index {idx}");
            Err(GeneralPalletError::DeadlineErrorDeadlineNotFound)
        }
    }

    /// Records a deadline as proven.
    ///
    /// Returns an error if the deadline has already been proven.
    pub fn record_proven(
        &mut self,
        deadline_idx: usize,
        all_sectors: &BoundedBTreeMap<
            SectorNumber,
            SectorOnChainInfo<BlockNumber>,
            ConstU32<MAX_SECTORS>,
        >,
        partitions: BoundedVec<PartitionNumber, ConstU32<MAX_PARTITIONS_PER_DEADLINE>>,
    ) -> Result<(), GeneralPalletError> {
        log::debug!(target: LOG_TARGET, "record_proven: partition number: {partitions:?}");
        let deadline = self.load_deadline_mut(deadline_idx)?;
        deadline.record_proven(all_sectors, partitions)?;
        Ok(())
    }
}

/// Holds information about deadlines like when they open and close and what deadline index they relate to.
///
/// Filecoin reference about PoSt deadline design:
/// <https://spec.filecoin.io/#section-algorithms.pos.post.design>
#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq, TypeInfo)]
pub struct DeadlineInfo<BlockNumber> {
    /// The block number at which this info was calculated.
    pub block_number: BlockNumber,

    /// The block number at which the proving period for this deadline starts.
    /// period_start < open_at to give time to SPs to create the proof before
    /// open.
    pub period_start: BlockNumber,

    /// The deadline index within its proving window.
    pub idx: u64,

    /// The first block number from which a proof can be submitted.
    /// open_at > period_start
    pub open_at: BlockNumber,

    /// The first block number from which a proof can *no longer* be submitted.
    pub close_at: BlockNumber,

    /// First block at which a fault declaration is rejected (< Open).
    pub fault_cutoff: BlockNumber,

    /// The block number at which the randomness for the deadline proving is
    /// available.
    pub challenge: BlockNumber,

    /// The number of non-overlapping PoSt deadlines in each proving period.
    pub w_post_period_deadlines: u64,

    /// The period over which all an SP's active sectors will be challenged.
    pub w_post_proving_period: BlockNumber,

    /// The duration of a deadline's challenge window. This is a window in which
    /// the storage provider can submit a PoSt for the deadline.
    pub w_post_challenge_window: BlockNumber,

    /// The duration of the lookback window for challenge responses. The period
    /// before a deadline when the randomness is available.
    pub w_post_challenge_lookback: BlockNumber,

    /// The fault declaration cutoff amount, consistent with FaultDeclarationCutoff.
    /// Stored here because it is used in the next_non_elapsed function.
    pub fault_declaration_cutoff: BlockNumber,
}

impl<BlockNumber> DeadlineInfo<BlockNumber>
where
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    /// Constructs a new `DeadlineInfo`.
    ///
    /// Reference: <https://github.com/filecoin-project/builtin-actors/blob/8d957d2901c0f2044417c268f0511324f591cb92/actors/miner/src/deadline_info.rs#L43>
    pub fn new(
        block_number: BlockNumber,
        period_start: BlockNumber,
        idx: u64,
        w_post_period_deadlines: u64,
        w_post_proving_period: BlockNumber,
        w_post_challenge_window: BlockNumber,
        w_post_challenge_lookback: BlockNumber,
        fault_declaration_cutoff: BlockNumber,
    ) -> Result<Self, GeneralPalletError> {
        // convert w_post_period_deadlines and idx so we can math
        let period_deadlines = BlockNumber::try_from(w_post_period_deadlines).map_err(|_| {
            log::error!(target: LOG_TARGET, "failed to convert {w_post_period_deadlines:?} to BlockNumber");
            GeneralPalletError::DeadlineErrorCouldNotConstructDeadlineInfo
        })?;

        let idx_converted = BlockNumber::try_from(idx).map_err(|_| {
            log::error!(target: LOG_TARGET, "failed to convert {idx:?} to BlockNumber");
            GeneralPalletError::DeadlineErrorCouldNotConstructDeadlineInfo
        })?;

        let (open_at, close_at, challenge, fault_cutoff) = if idx_converted < period_deadlines {
            let open_at = period_start + (idx_converted * w_post_challenge_window);
            let close_at = open_at + w_post_challenge_window;
            let challenge = period_start - w_post_challenge_lookback;
            let fault_cutoff = open_at - fault_declaration_cutoff;
            (open_at, close_at, challenge, fault_cutoff)
        } else {
            let after_last_deadline = period_start + w_post_proving_period;
            (
                after_last_deadline,
                after_last_deadline,
                after_last_deadline,
                BlockNumber::zero(),
            )
        };

        Ok(Self {
            block_number,
            period_start,
            idx,
            open_at,
            close_at,
            challenge,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_cutoff,
            fault_declaration_cutoff,
        })
    }

    /// Whether the proving period has begun.
    pub fn period_started(&self) -> bool {
        self.block_number >= self.period_start
    }

    /// Whether the current deadline is currently open.
    pub fn is_open(&self) -> bool {
        self.block_number >= self.open_at && self.block_number < self.close_at
    }

    /// Whether the current deadline has already closed.
    pub fn has_elapsed(&self) -> bool {
        self.block_number >= self.close_at
    }

    /// The last block during which a proof may be submitted.
    ///
    /// When the value of `close_at` is 0 this function will also return 0 instead of panicking or underflowing.
    pub fn last(&self) -> BlockNumber {
        self.close_at.saturating_less_one()
    }

    /// Whether the deadline's fault cutoff has passed.
    pub fn fault_cutoff_passed(&self) -> bool {
        self.block_number >= self.fault_cutoff
    }

    /// Returns the next deadline that has not yet elapsed.
    ///
    /// If the current deadline has not elapsed yet then it returns the current deadline.
    /// Otherwise it calculates the next period start by getting the gap between the current block number and the closing block number
    /// and adding 1. Making sure it is a multiple of proving period by dividing by `w_post_proving_period`.
    pub fn next_not_elapsed(self) -> Result<Self, GeneralPalletError> {
        if !self.has_elapsed() {
            return Ok(self);
        }

        // has elapsed, advance by some multiples of w_post_proving_period
        let gap = self.block_number - self.close_at;
        let delta_periods = BlockNumber::one() + gap / self.w_post_proving_period;

        Self::new(
            self.block_number,
            self.period_start + self.w_post_proving_period * delta_periods,
            self.idx,
            self.w_post_period_deadlines,
            self.w_post_proving_period,
            self.w_post_challenge_window,
            self.w_post_challenge_lookback,
            self.fault_declaration_cutoff,
        )
    }
}

/// Returns true if the deadline at the given index is currently mutable.
///
/// Deadlines are considered to be immutable if they are being proven or about to be proven.
///
/// Reference: <https://spec.filecoin.io/#example-storage-miner-actor>
pub fn deadline_is_mutable<BlockNumber>(
    proving_period_start: BlockNumber,
    deadline_idx: u64,
    current_block: BlockNumber,
    w_post_period_deadlines: u64,
    w_post_proving_period: BlockNumber,
    w_post_challenge_window: BlockNumber,
    w_post_challenge_lookback: BlockNumber,
    fault_declaration_cutoff: BlockNumber,
) -> Result<bool, GeneralPalletError>
where
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    // Get the next non-elapsed deadline (i.e., the next time we care about
    // mutations to the deadline).
    let dl_info = DeadlineInfo::new(
        current_block,
        proving_period_start,
        deadline_idx,
        w_post_period_deadlines,
        w_post_proving_period,
        w_post_challenge_window,
        w_post_challenge_lookback,
        fault_declaration_cutoff,
    )?
    .next_not_elapsed()?;

    // Ensure that the current block is at least one challenge window before
    // that deadline opens.
    Ok(current_block < dl_info.open_at - w_post_challenge_window)
}
