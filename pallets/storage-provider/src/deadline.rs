use codec::{Decode, Encode};
use frame_support::{
    pallet_prelude::*,
    sp_runtime::{BoundedBTreeMap, BoundedVec},
    PalletError,
};
use primitives_proofs::SectorNumber;
use scale_info::{
    prelude::{cmp, vec::Vec},
    TypeInfo,
};
use sp_arithmetic::traits::BaseArithmetic;

use crate::{
    pallet::LOG_TARGET,
    partition::{Partition, PartitionNumber, MAX_PARTITIONS},
    sector::SectorOnChainInfo,
};

mod assignment;

pub use assignment::assign_deadlines;

type DeadlineResult<T> = Result<T, DeadlineError>;

/// Deadline holds the state for all sectors due at a specific deadline.
///
/// A deadline exists along side 47 other deadlines (1 for every 30 minutes in a day).
/// Only one deadline may be active for a given proving window.
#[derive(Clone, Debug, Decode, Encode, PartialEq, TypeInfo)]
pub struct Deadline<BlockNumber> {
    /// Partitions in this deadline. Indexed on partition number.
    pub partitions:
        BoundedBTreeMap<PartitionNumber, Partition<BlockNumber>, ConstU32<MAX_PARTITIONS>>,

    /// Maps blocks to partitions that _may_ have sectors that expire in or
    /// before that block, either on-time or early as faults.
    /// Keys are quantized to final blocks in each proving deadline.
    ///
    /// NOTE: Partitions MUST NOT be removed from this queue (until the
    /// associated block has passed) even if they no longer have sectors
    /// expiring at that block. Sectors expiring at this block may later be
    /// recovered, and this queue will not be updated at that time.
    pub expirations_blocks: BoundedBTreeMap<BlockNumber, PartitionNumber, ConstU32<MAX_PARTITIONS>>,

    /// Partitions that have been proved by window PoSts so far during the
    /// current challenge window.
    pub partitions_posted: BoundedBTreeSet<PartitionNumber, ConstU32<MAX_PARTITIONS>>,

    /// Partition numbers with sectors that terminated early.
    pub early_terminations: BoundedBTreeSet<PartitionNumber, ConstU32<MAX_PARTITIONS>>,

    /// The number of non-terminated sectors in this deadline (incl faulty).
    pub live_sectors: u64,

    /// The total number of sectors in this deadline (incl dead).
    pub total_sectors: u64,
}

impl<BlockNumber: Clone + Copy + Ord> Deadline<BlockNumber> {
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

    pub fn update_deadline(&mut self, new_dl: Self) {
        self.partitions_posted = new_dl.partitions_posted;
        self.expirations_blocks = new_dl.expirations_blocks;
        self.early_terminations = new_dl.early_terminations;
        self.live_sectors = new_dl.live_sectors;
        self.total_sectors = new_dl.total_sectors;
        self.partitions = new_dl.partitions;
    }

    /// Processes a PoSt
    pub fn record_proven(&mut self, partition_num: PartitionNumber) -> DeadlineResult<()> {
        log::debug!(target: LOG_TARGET, "record_proven: partition number = {partition_num:?}");
        ensure!(
            !self.partitions_posted.contains(&partition_num),
            DeadlineError::PartitionAlreadyProven
        );
        self.partitions_posted
            .try_insert(partition_num)
            .map_err(|_| DeadlineError::ProofUpdateFailed)?;
        Ok(())
    }

    /// Adds sectors to a deadline. It's the caller's responsibility to make sure
    /// that this deadline isn't currently "open" (i.e., being proved at this point
    /// in time).
    /// The sectors are assumed to be non-faulty.
    ///
    /// The sectors are added to the last partition stored in the deadline. .
    pub fn add_sectors(
        &mut self,
        partition_size: u64,
        mut sectors: &[SectorOnChainInfo<BlockNumber>],
    ) -> DeadlineResult<()> {
        if sectors.is_empty() {
            return Ok(());
        }

        // First update partitions, consuming the sectors
        let mut partition_deadline_updates =
            Vec::<(BlockNumber, PartitionNumber)>::with_capacity(sectors.len());
        self.live_sectors += sectors.len() as u64;
        self.total_sectors += sectors.len() as u64;

        let partitions = &mut self.partitions;

        // try filling up the last partition first.
        for partition_idx in partitions.iter().count().saturating_sub(1).. {
            // Get/create partition to update.
            let mut partition = match partitions.get_mut(&(partition_idx as u32)) {
                Some(partition) => partition.clone(),
                None => {
                    // This case will usually happen zero times.
                    // It would require adding more than a full partition in one go
                    // to happen more than once.
                    let partition = Partition::new();
                    partition
                }
            };

            // Figure out which (if any) sectors we want to add to this partition.
            let sector_count = partition.sectors.len() as u64;
            if sector_count >= partition_size {
                continue;
            }

            let size = cmp::min(partition_size - sector_count, sectors.len() as u64) as usize;
            let partition_new_sectors = &sectors[..size];

            // Intentionally ignoring the index at size, split_at returns size inclusively for start
            sectors = &sectors[size..];

            let new_partition_sectors: Vec<SectorNumber> = partition_new_sectors
                .into_iter()
                .map(|sector| sector.sector_number)
                .collect();

            // Add sectors to partition.
            partition
                .add_sectors(&new_partition_sectors)
                .map_err(|_| DeadlineError::CouldNotAddSectors)?;

            // Save partition back.
            match partitions.get_mut(&(partition_idx as u32)) {
                Some(p) => *p = partition,
                None => {
                    let _ = partitions.try_insert(partition_idx as u32, partition);
                }
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
        }

        // Next, update the expiration queue.
        for (block, partition_index) in partition_deadline_updates {
            let _ = self.expirations_blocks.try_insert(block, partition_index);
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, TypeInfo)]
pub struct Deadlines<BlockNumber> {
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

impl<BlockNumber: Clone + Copy + Ord> Deadlines<BlockNumber> {
    /// Constructor function.
    pub fn new() -> Self {
        let mut due = BoundedVec::new();
        // Initialize deadlines as empty deadlines.
        for _ in 0..48 {
            let _ = due.try_push(Deadline::new());
        }
        Self { due }
    }

    /// Get the amount of deadlines that are due.
    pub fn len(&self) -> usize {
        self.due.len()
    }

    /// Inserts a new deadline.
    /// Fails if the deadline insertion fails.
    /// Returns the deadline index it inserted the deadline at
    ///
    /// I am not sure if this should just insert the new deadline at the back and return the index
    /// or take in the index and insert the deadline in there.
    pub fn insert_deadline(
        &mut self,
        new_deadline: Deadline<BlockNumber>,
    ) -> DeadlineResult<usize> {
        self.due
            .try_push(new_deadline)
            .map_err(|_| DeadlineError::CouldNotInsertDeadline)?;
        // No underflow if the above was successful, minimum length 1
        Ok(self.due.len() - 1)
    }

    /// Loads a mutable deadline from the given index.
    /// Fails if the index does not exist or is out of range.
    pub fn load_deadline_mut(&mut self, idx: usize) -> DeadlineResult<&mut Deadline<BlockNumber>> {
        log::debug!(target: LOG_TARGET, "load_deadline_mut: getting deadline at index {idx}");
        // Ensure the provided index is within range.
        ensure!(self.len() > idx, DeadlineError::DeadlineIndexOutOfRange);
        self.due.get_mut(idx).ok_or(DeadlineError::DeadlineNotFound)
    }

    /// Loads a deadline
    /// Fails if the index does not exist or is out of range.
    pub fn load_deadline(&self, idx: usize) -> DeadlineResult<Deadline<BlockNumber>> {
        log::debug!(target: LOG_TARGET, "load_deadline_mut: getting deadline at index {idx}");
        // Ensure the provided index is within range.
        ensure!(self.len() > idx, DeadlineError::DeadlineIndexOutOfRange);
        self.due
            .get(idx)
            .cloned()
            .ok_or(DeadlineError::DeadlineNotFound)
    }

    /// Records a deadline as proven
    pub fn record_proven(
        &mut self,
        deadline_idx: usize,
        partition_num: PartitionNumber,
    ) -> DeadlineResult<()> {
        log::debug!(target: LOG_TARGET, "record_proven: partition number: {partition_num:?}");
        let deadline = self.load_deadline_mut(deadline_idx)?;
        deadline.record_proven(partition_num)?;
        Ok(())
    }

    pub fn for_each(
        &self,
        mut f: impl FnMut(u64, Deadline<BlockNumber>) -> DeadlineResult<()>,
    ) -> DeadlineResult<()> {
        for i in 0..(self.len() as u64) {
            let index = i;
            let deadline = self.load_deadline(i as usize)?;
            f(index, deadline)?;
        }
        Ok(())
    }

    pub fn update_deadline(
        &mut self,
        deadline_idx: usize,
        deadline: Deadline<BlockNumber>,
    ) -> DeadlineResult<()> {
        let dl = self
            .due
            .get_mut(deadline_idx)
            .ok_or(DeadlineError::DeadlineNotFound)?;
        dl.update_deadline(deadline);
        Ok(())
    }
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, TypeInfo)]
pub struct DeadlineInfo<BlockNumber> {
    /// The block number at which this info was calculated.
    pub block_number: BlockNumber,

    /// The block number at which the proving period for this deadline starts.
    pub period_start: BlockNumber,

    /// The deadline index within its proving window.
    pub idx: u64,

    /// The first block number from which a proof can be submitted.
    pub open: BlockNumber,

    /// The first block number from which a proof can *no longer* be submitted.
    pub close: BlockNumber,

    pub w_post_period_deadlines: u64,
    pub w_post_proving_period: BlockNumber,
    pub w_post_challenge_window: BlockNumber,
}

impl<BlockNumber: BaseArithmetic + Copy> DeadlineInfo<BlockNumber> {
    /// Constructs a new `DeadlineInfo`
    // ref: <https://github.com/filecoin-project/builtin-actors/blob/8d957d2901c0f2044417c268f0511324f591cb92/actors/miner/src/deadline_info.rs#L43>
    pub fn new(
        block_number: BlockNumber,
        period_start: BlockNumber,
        idx: u64,
        w_post_period_deadlines: u64,
        w_post_challenge_window: BlockNumber,
        w_post_proving_period: BlockNumber,
    ) -> DeadlineResult<Self> {
        // convert w_post_period_deadlines and idx so we can math
        // interesting that the error type for `BlockNumber::try_from` is `Infallible` indicating that it cannot fail.
        // ref: <https://doc.rust-lang.org/nightly/core/convert/trait.TryFrom.html#generic-implementations>
        // does this mean we do no need to catch the error?
        let period_deadlines = BlockNumber::try_from(w_post_period_deadlines)
            .map_err(|_| DeadlineError::CouldNotConstructDeadlineInfo)?;
        let idx_converted =
            BlockNumber::try_from(idx).map_err(|_| DeadlineError::CouldNotConstructDeadlineInfo)?;
        if idx_converted < period_deadlines {
            let deadline_open = period_start + (idx_converted * w_post_challenge_window);
            Ok(Self {
                block_number,
                period_start,
                idx,
                open: deadline_open,
                close: deadline_open + w_post_challenge_window,
                w_post_period_deadlines,
                w_post_challenge_window,
                w_post_proving_period,
            })
        } else {
            let after_last_deadline = period_start + w_post_proving_period;
            Ok(Self {
                block_number,
                period_start,
                idx,
                open: after_last_deadline,
                close: after_last_deadline,
                w_post_period_deadlines,
                w_post_challenge_window,
                w_post_proving_period,
            })
        }
    }

    /// Whether the current deadline is currently open.
    pub fn is_open(&self) -> bool {
        self.block_number >= self.open && self.block_number < self.close
    }

    /// Whether the current deadline has already closed.
    pub fn has_elapsed(&self) -> bool {
        self.block_number >= self.close
    }

    /// Returns the next instance of this deadline that has not yet elapsed.
    pub fn next_not_elapsed(self) -> DeadlineResult<Self> {
        if !self.has_elapsed() {
            return Ok(self);
        }

        // has elapsed, advance by some multiples of w_post_proving_period
        let gap = self.block_number - self.close;
        let delta_periods = TryInto::<BlockNumber>::try_into(1u64)
            .map_err(|_| DeadlineError::FailedToGetNextDeadline)?
            + gap / self.w_post_proving_period;

        Self::new(
            self.block_number,
            self.period_start + self.w_post_proving_period * delta_periods,
            self.idx,
            self.w_post_period_deadlines,
            self.w_post_proving_period,
            self.w_post_challenge_window,
        )
    }
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
    /// Emitted when trying to get the next instance of a deadline that has not yet elapsed fails.
    FailedToGetNextDeadline,
    /// Emitted when max partition for a given deadline have been reached.
    MaxPartitionsReached,
    /// Emitted when trying to add sectors to a deadline fails.
    CouldNotAddSectors,
    /// Emitted when assigning sectors to deadlines fails.
    CouldNotAssignSectorsToDeadlines,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn load_deadline_mut() -> DeadlineResult<()> {
        let mut dls: Deadlines<u32> = Deadlines::new();
        let dl: Deadline<u32> = Deadline::new();

        let idx = dls.insert_deadline(dl.clone())?;
        let loaded_dl = dls.load_deadline_mut(idx)?;
        assert_eq!(&dl, loaded_dl);
        Ok(())
    }
}
