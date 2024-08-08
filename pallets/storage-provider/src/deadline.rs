extern crate alloc;

use alloc::vec::Vec;

use codec::{Decode, Encode};
use frame_support::{pallet_prelude::*, sp_runtime::BoundedBTreeMap, PalletError};
use primitives_proofs::SectorNumber;
use scale_info::{prelude::cmp, TypeInfo};

use crate::{
    partition::{Partition, PartitionError, PartitionNumber, MAX_PARTITIONS_PER_DEADLINE},
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
pub struct Deadline<BlockNumber: sp_runtime::traits::BlockNumber> {
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
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    /// Construct a new [`Deadline`] instance.
    pub fn new() -> Self {
        let mut partitions = BoundedBTreeMap::new();
        for partition_number in 0..=MAX_PARTITIONS_PER_DEADLINE {
            let _ = partitions.try_insert(partition_number, Partition::new());
        }
        Self {
            partitions,
            expirations_blocks: BoundedBTreeMap::new(),
            partitions_posted: BoundedBTreeSet::new(),
            early_terminations: BoundedBTreeSet::new(),
            live_sectors: 0,
            total_sectors: 0,
        }
    }

    /// Sets a given partition as proven.
    ///
    /// If the partition has already been proven, an error is returned.
    pub fn record_proven(&mut self, partition_num: PartitionNumber) -> Result<(), DeadlineError> {
        log::debug!(target: LOG_TARGET, "record_proven: partition number = {partition_num:?}");

        // Ensure the partition exists.
        ensure!(self.partitions.contains_key(&partition_num), {
            log::error!(target: LOG_TARGET, "record_proven: partition {partition_num:?} not found");
            DeadlineError::PartitionNotFound
        });

        // Ensure the partition hasn't already been proven.
        ensure!(!self.partitions_posted.contains(&partition_num), {
            log::error!(target: LOG_TARGET, "record_proven: partition {partition_num:?} already proven");
            DeadlineError::PartitionAlreadyProven
        });

        // Record the partition as proven.
        self.partitions_posted
            .try_insert(partition_num)
            .map_err(|_| DeadlineError::ProofUpdateFailed)?;

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
    ) -> Result<(), DeadlineError> {
        if sectors.is_empty() {
            return Ok(());
        }

        // First update partitions, consuming the sectors
        let mut partition_deadline_updates =
            Vec::<(BlockNumber, PartitionNumber)>::with_capacity(sectors.len());
        // PRE-COND: there can never be more live sectors than u64, so it never overflows
        self.live_sectors += sectors.len() as u64;
        self.total_sectors += sectors.len() as u64;

        let partitions = &mut self.partitions;

        // Needs to start at 1 because the length is constants
        let mut partition_idx = 1;
        loop {
            // Get/create partition to update.
            let mut partition = match partitions.get_mut(&(partition_idx as u32)) {
                Some(partition) => partition.clone(),
                None => {
                    // This case will only happen when trying to add a full partition more than once in go.
                    Partition::new()
                }
            };

            // Figure out which (if any) sectors we want to add to this partition.
            let sector_count = partition.sectors.len() as u64;
            if sector_count >= partition_size {
                continue;
            }

            let size = cmp::min(partition_size - sector_count, sectors.len() as u64) as usize;

            let (partition_new_sectors, sectors) = sectors.split_at(size);

            let new_partition_sectors: Vec<SectorNumber> = partition_new_sectors
                .into_iter()
                .map(|sector| sector.sector_number)
                .collect();

            // Add sectors to partition.
            partition
                .add_sectors(&new_partition_sectors)
                .map_err(|_| DeadlineError::CouldNotAddSectors)?;

            // Save partition if it is newly constructed.
            if !partitions.contains_key(&(partition_idx as u32)) {
                partitions.try_insert(partition_idx as u32, partition).map_err(|_| {
                    log::error!(target: LOG_TARGET, "add_sectors: Cannot insert new partition at {partition_idx}");
                    DeadlineError::CouldNotAddSectors
                })?;
            }

            // Record deadline -> partition mapping so we can later update the deadlines.
            partition_deadline_updates.extend(
                partition_new_sectors
                    .iter()
                    .map(|s| (s.expiration, partition_idx as PartitionNumber)),
            );

            if sectors.is_empty() {
                break;
            }
            partition_idx += 1;
        }

        // Next, update the expiration queue.
        for (block, partition_index) in partition_deadline_updates {
            self.expirations_blocks.try_insert(block, partition_index).map_err(|_| {
                log::error!(target: LOG_TARGET, "add_sectors: Cannot update expiration queue at index {partition_idx}");
                DeadlineError::CouldNotAddSectors
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
    ) -> Result<(), DeadlineError> {
        for (partition_number, partition) in self.partitions.iter_mut() {
            if !partition_sectors.0.contains_key(&partition_number) {
                continue;
            }
            partition.record_faults(
                sectors,
                partition_sectors
                .0
                .get(partition_number)
                .expect("Infallible because of the above check"),
            ).map_err(|e| {
                log::error!(target: LOG_TARGET, "record_faults: Error while recording faults in a partition: {e:?}");
                DeadlineError::PartitionError(e)
            })?;
            // Update expiration block
            if let Some((block, _)) = self
                .expirations_blocks
                .iter()
                .find(|(_, partition_num)| partition_num == &partition_number)
            {
                self.expirations_blocks.remove(&block.clone());
                self.expirations_blocks.try_insert(fault_expiration_block, *partition_number).map_err(|_| {
                        log::error!(target: LOG_TARGET, "record_faults: Could not insert new expiration");
                        DeadlineError::FailedToUpdateFaultExpiration
                    })?;
            } else {
                log::debug!(target: LOG_TARGET, "record_faults: Inserting partition number {partition_number}");
                self.expirations_blocks.try_insert(fault_expiration_block, *partition_number).map_err(|_| {
                    log::error!(target: LOG_TARGET, "record_faults: Could not insert new expiration");
                    DeadlineError::FailedToUpdateFaultExpiration
                })?;
            }
        }

        Ok(())
    }
}

#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq, TypeInfo)]
pub struct Deadlines<BlockNumber: sp_runtime::traits::BlockNumber> {
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
    ) -> Result<&mut Deadline<BlockNumber>, DeadlineError> {
        log::debug!(target: LOG_TARGET, "load_deadline_mut: getting deadline at index {idx}");
        // Ensure the provided index is within range.
        ensure!(self.len() > idx, DeadlineError::DeadlineIndexOutOfRange);
        Ok(self
            .due
            .get_mut(idx)
            .expect("Deadlines are pre-initialized, this cannot fail"))
    }

    /// Loads a deadline
    /// Fails if the index does not exist or is out of range.
    pub fn load_deadline(&self, idx: usize) -> Result<Deadline<BlockNumber>, DeadlineError> {
        log::debug!(target: LOG_TARGET, "load_deadline_mut: getting deadline at index {idx}");
        // Ensure the provided index is within range.
        ensure!(self.len() > idx, DeadlineError::DeadlineIndexOutOfRange);
        Ok(self
            .due
            .get(idx)
            .cloned()
            .expect("Deadlines are pre-initialized, this cannot fail"))
    }

    /// Records a deadline as proven.
    ///
    /// Returns an error if the deadline has already been proven.
    pub fn record_proven(
        &mut self,
        deadline_idx: usize,
        partition_num: PartitionNumber,
    ) -> Result<(), DeadlineError> {
        log::debug!(target: LOG_TARGET, "record_proven: partition number: {partition_num:?}");
        let deadline = self.load_deadline_mut(deadline_idx)?;
        deadline.record_proven(partition_num)?;
        Ok(())
    }

    /// Replace values of the deadline at index `deadline_idx` with those of `new_dl`.
    ///
    /// IMPORTANT: It is the caller of this functions responsibility to make sure the given index exists.
    pub fn update_deadline(&mut self, deadline_idx: usize, new_dl: Deadline<BlockNumber>) {
        self.due[deadline_idx] = new_dl;
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
        fault_declaration_cutoff: BlockNumber,
        w_post_period_deadlines: u64,
        w_post_proving_period: BlockNumber,
        w_post_challenge_window: BlockNumber,
        w_post_challenge_lookback: BlockNumber,
    ) -> Result<Self, DeadlineError> {
        // convert w_post_period_deadlines and idx so we can math
        let period_deadlines = BlockNumber::try_from(w_post_period_deadlines).map_err(|_| {
            log::error!(target: LOG_TARGET, "failed to convert {w_post_period_deadlines:?} to BlockNumber");
            DeadlineError::CouldNotConstructDeadlineInfo
        })?;

        let idx_converted = BlockNumber::try_from(idx).map_err(|_| {
            log::error!(target: LOG_TARGET, "failed to convert {idx:?} to BlockNumber");
            DeadlineError::CouldNotConstructDeadlineInfo
        })?;

        let (open_at, close_at, challenge, fault_cutoff) = if idx_converted < period_deadlines {
            let open_at = period_start + (idx_converted * w_post_challenge_window);
            let close_at = open_at + w_post_challenge_window;
            let challenge = period_start - w_post_challenge_lookback;
            let fault_cutoff = open_at + fault_declaration_cutoff;
            (open_at, close_at, challenge, fault_cutoff)
        } else {
            let after_last_deadline = period_start + w_post_proving_period;
            (
                after_last_deadline,
                after_last_deadline,
                BlockNumber::zero(),
                after_last_deadline,
            )
        };

        Ok(Self {
            block_number,
            period_start,
            idx,
            open_at,
            close_at,
            fault_cutoff,
            challenge,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
        })
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
    pub fn next_not_elapsed(self) -> Result<Self, DeadlineError> {
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
            self.fault_cutoff,
            self.w_post_period_deadlines,
            self.w_post_proving_period,
            self.w_post_challenge_window,
            self.w_post_challenge_lookback,
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
    fault_declaration_cutoff: BlockNumber,
    w_post_period_deadlines: u64,
    w_post_proving_period: BlockNumber,
    w_post_challenge_window: BlockNumber,
    w_post_challenge_lookback: BlockNumber,
) -> Result<bool, DeadlineError>
where
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    // Get the next non-elapsed deadline (i.e., the next time we care about
    // mutations to the deadline).
    let dl_info = DeadlineInfo::new(
        current_block,
        proving_period_start,
        deadline_idx,
        fault_declaration_cutoff,
        w_post_period_deadlines,
        w_post_proving_period,
        w_post_challenge_window,
        w_post_challenge_lookback,
    )?
    .next_not_elapsed()?;

    // Ensure that the current block is at least one challenge window before
    // that deadline opens.
    Ok(current_block < dl_info.open_at - w_post_challenge_window)
}

#[derive(Decode, Encode, PalletError, TypeInfo, RuntimeDebug)]
pub enum DeadlineError {
    /// Emitted when the passed in deadline index supplied for `submit_windowed_post` is out of range.
    DeadlineIndexOutOfRange,
    /// Emitted when a trying to get a deadline index but fails because that index does not exist.
    DeadlineNotFound,
    /// Emitted when a given index in `Deadlines` already exists and try to insert a deadline on that index.
    DeadlineIndexExists,
    /// Emitted when trying to insert a new deadline fails.
    CouldNotInsertDeadline,
    /// Emitted when constructing `DeadlineInfo` fails.
    CouldNotConstructDeadlineInfo,
    /// Emitted when a proof is submitted for a partition that is already proven.
    PartitionAlreadyProven,
    /// Emitted when trying to retrieve a partition that does not exit.
    PartitionNotFound,
    /// Emitted when trying to update proven partitions fails.
    ProofUpdateFailed,
    /// Emitted when max partition for a given deadline have been reached.
    MaxPartitionsReached,
    /// Emitted when trying to add sectors to a deadline fails.
    CouldNotAddSectors,
    /// Emitted when assigning sectors to deadlines fails.
    CouldNotAssignSectorsToDeadlines,
    /// Emitted when updates to a partition fail.
    FailedToUpdatePartition,
    /// Emitted when trying to update a deadline fails.
    FailedToUpdateDeadline,
    /// Emitted when trying to update fault expirations fails
    FailedToUpdateFaultExpiration,
    /// Wrapper around the partition error type
    PartitionError(PartitionError),
}
