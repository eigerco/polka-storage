extern crate alloc;
use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use core::ops::Not;

use codec::{Decode, Encode};
use frame_support::PalletError;
use primitives_proofs::SectorNumber;
use scale_info::TypeInfo;
use sp_core::{ConstU32, RuntimeDebug};
use sp_runtime::{BoundedBTreeMap, BoundedBTreeSet};

use crate::sector::{SectorOnChainInfo, MAX_SECTORS};

const LOG_TARGET: &'static str = "runtime::storage_provider::expiration_queue";

/// ExpirationSet is a collection of sector numbers that are expiring, either
/// due to expected "on-time" expiration at the end of their life, or unexpected
/// "early" termination due to being faulty for too long consecutively.
#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq, TypeInfo)]
struct ExpirationSet {
    /// Sectors expiring "on time" at the end of their committed life
    on_time_sectors: BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,
    /// Sectors expiring "early" due to being faulty for too long
    early_sectors: BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,
}

impl ExpirationSet {
    pub fn new() -> Self {
        Self {
            on_time_sectors: BoundedBTreeSet::new(),
            early_sectors: BoundedBTreeSet::new(),
        }
    }

    /// Adds sectors to the expiration set.
    pub fn add(
        &mut self,
        on_time_sectors: &[SectorNumber],
        early_sectors: &[SectorNumber],
    ) -> Result<(), ExpirationQueueError> {
        for sector in on_time_sectors {
            self.on_time_sectors
                .try_insert(*sector)
                .map_err(|_| ExpirationQueueError::InsertionFailed)?;
        }

        for sector in early_sectors {
            self.early_sectors
                .try_insert(*sector)
                .map_err(|_| ExpirationQueueError::InsertionFailed)?;
        }

        Ok(())
    }

    /// Removes sectors from the expiration set.
    ///
    /// Operation is a no-op if the sector is not in the set.
    pub fn remove(&mut self, on_time_sectors: &[SectorNumber], early_sectors: &[SectorNumber]) {
        for sector in on_time_sectors {
            self.on_time_sectors.remove(sector);
        }

        for sector in early_sectors {
            self.early_sectors.remove(sector);
        }
    }

    /// A set is empty if it has no sectors.
    pub fn is_empty(&self) -> bool {
        self.on_time_sectors.is_empty() && self.early_sectors.is_empty()
    }

    /// Counts all sectors in the expiration set.
    pub fn _len(&self) -> usize {
        self.on_time_sectors.len() + self.early_sectors.len()
    }
}

/// ExpirationQueue represents a queue of sector expirations.
///
/// It maintains a map of block numbers to expiration sets, where each
/// expiration set contains sectors that are due to expire at that block.
///
/// The queue is bounded by the maximum number of sectors that can be stored
/// by a single storage provider. This means that the map could hold max
/// sectors even if each sector would expire in each own block.
#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq, TypeInfo)]
pub struct ExpirationQueue<BlockNumber>
where
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    map: BoundedBTreeMap<BlockNumber, ExpirationSet, ConstU32<MAX_SECTORS>>,
}

impl<BlockNumber> ExpirationQueue<BlockNumber>
where
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    pub fn new() -> Self {
        Self {
            map: BoundedBTreeMap::new(),
        }
    }

    /// Adds a collection of sectors to their on-time target expiration entries.
    /// The sectors are assumed to be active (non-faulty).
    ///
    /// https://github.com/filecoin-project/builtin-actors/blob/c3c41c5d06fe78c88d4d05eb81b749a6586a5c9f/actors/miner/src/expiration_queue.rs#L171
    pub fn add_active_sectors(
        &mut self,
        sectors: &[SectorOnChainInfo<BlockNumber>],
    ) -> Result<(), ExpirationQueueError> {
        for sector in sectors {
            self.add_to_expiration_set(sector.expiration, &[sector.sector_number], &[])?;
        }

        Ok(())
    }

    /// Re-schedules sectors to expire at an early expiration height, if they
    /// wouldn't expire before then anyway. The sectors must not be currently
    /// faulty, so must be registered as expiring on-time rather than early. The
    /// pledge for the now-early sectors is removed from the queue.
    ///
    /// https://github.com/filecoin-project/builtin-actors/blob/c3c41c5d06fe78c88d4d05eb81b749a6586a5c9f/actors/miner/src/expiration_queue.rs#L237
    pub fn reschedule_as_faults(
        &mut self,
        new_expiration: BlockNumber,
        sectors: &[&SectorOnChainInfo<BlockNumber>],
    ) -> Result<(), ExpirationQueueError> {
        // Sectors grouped by the expiration they currently have.
        let groups = self.find_sectors_by_expiration(sectors)?;

        // Sectors that are rescheduled for early expiration
        let mut early_sectors = Vec::new();

        // Remove sectors from active
        for (expiration_height, mut set) in groups {
            if expiration_height <= new_expiration {
                // Sector is already expiring before the new expiration height,
                // so it can't be rescheduled as faulty.
                continue;
            } else {
                // Remove sectors from on-time expiry
                set.expiration_set.remove(&set.sectors, &[]);
                early_sectors.extend(set.sectors);
            }

            self.must_update_or_delete(expiration_height, set.expiration_set)?;
        }

        // Reschedule faulty sectors
        self.add_to_expiration_set(new_expiration, &[], &early_sectors)?;

        Ok(())
    }

    /// Removes sectors from any queue entries in which they appear that are
    /// earlier then their scheduled expiration height, and schedules them at
    /// their expected termination height.
    ///
    /// https://github.com/filecoin-project/builtin-actors/blob/c3c41c5d06fe78c88d4d05eb81b749a6586a5c9f/actors/miner/src/expiration_queue.rs#L361
    pub fn reschedule_recovered(
        &mut self,
        all_sectors: &BoundedBTreeMap<
            SectorNumber,
            SectorOnChainInfo<BlockNumber>,
            ConstU32<MAX_SECTORS>,
        >,
        reschedule: &BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,
    ) -> Result<(), ExpirationQueueError> {
        // Sectors remaining to be rescheduled.
        let mut _remaining = reschedule
            .iter()
            .map(|s| (s, all_sectors.get(s).unwrap()))
            .collect::<BTreeMap<_, _>>();

        // TODO(no-ref,@cernicc,17/09/2024): Implement rescheduling of recovered sectors
        Ok(())
    }

    /// Remove some sectors from the queue. The sectors may be active or faulty,
    /// and scheduled either for on-time or early termination. Fails if any
    /// sectors are not found in the queue.
    pub fn remove_sectors(
        _sectors: &[SectorOnChainInfo<BlockNumber>],
    ) -> Result<(), ExpirationQueueError> {
        todo!()
    }

    /// Add sectors to the specific expiration set. The operation is a no-op if
    /// both slices are empty.
    ///
    /// https://github.com/filecoin-project/builtin-actors/blob/c3c41c5d06fe78c88d4d05eb81b749a6586a5c9f/actors/miner/src/expiration_queue.rs#L626
    fn add_to_expiration_set(
        &mut self,
        expiration: BlockNumber,
        on_time_sectors: &[SectorNumber],
        early_sectors: &[SectorNumber],
    ) -> Result<(), ExpirationQueueError> {
        if on_time_sectors.is_empty() && early_sectors.is_empty() {
            return Ok(());
        }

        let mut expiration_set = self
            .map
            .get(&expiration)
            .cloned()
            .unwrap_or_else(|| ExpirationSet::new());

        // Add sectors to a set
        expiration_set.add(on_time_sectors, early_sectors)?;

        self.map
            .try_insert(expiration, expiration_set)
            .map_err(|_| ExpirationQueueError::InsertionFailed)?;

        Ok(())
    }

    /// Removes active sectors from the queue.
    ///
    /// https://github.com/filecoin-project/builtin-actors/blob/c3c41c5d06fe78c88d4d05eb81b749a6586a5c9f/actors/miner/src/expiration_queue.rs#L672
    fn _remove_active_sectors(
        &mut self,
        sectors: &[&SectorOnChainInfo<BlockNumber>],
    ) -> Result<(), ExpirationQueueError> {
        // Group sectors by their expiration, then remove from existing queue
        // entries according to those groups.
        let groups = self.find_sectors_by_expiration(sectors)?;

        for (expiration_height, set) in groups {
            let mut expiration_set = self
                .map
                .get(&expiration_height)
                .ok_or(ExpirationQueueError::ExpirationSetNotFound)?
                .clone();

            // Remove sectors from the set
            expiration_set.remove(&set.sectors, &[]);

            // Update the expiration set
            self.must_update_or_delete(expiration_height, expiration_set)?;
        }

        Ok(())
    }

    /// Updates the expiration set for a given expiration block number, or
    /// removes it if the set becomes empty.
    fn must_update_or_delete(
        &mut self,
        expiration: BlockNumber,
        expiration_set: ExpirationSet,
    ) -> Result<(), ExpirationQueueError> {
        if expiration_set.is_empty() {
            self.map.remove(&expiration);
        } else {
            self.map
                .try_insert(expiration, expiration_set)
                .map_err(|_| ExpirationQueueError::InsertionFailed)?;
        }

        Ok(())
    }

    /// Groups sectors into sets based on their Expiration field. If sectors are
    /// not found in the expiration set corresponding to their expiration field
    /// (i.e. they have been rescheduled) traverse expiration sets for groups
    /// where these sectors actually expire. Groups will be returned in
    /// expiration order, earliest first.
    ///
    /// Note: The function only searches for active sectors.
    fn find_sectors_by_expiration(
        &self,
        sectors: &[&SectorOnChainInfo<BlockNumber>],
    ) -> Result<BTreeMap<BlockNumber, SectorExpirationSet>, ExpirationQueueError> {
        // `declared_expirations` expirations we are searching for sectors in
        // `all_remaining` sector numbers we are searching for. we are removing
        // them from the set when we find them
        let (declared_expirations, mut all_remaining) = sectors.iter().fold(
            (BTreeSet::new(), BTreeSet::new()),
            |(mut expirations, mut remaining), sector| {
                expirations.insert(sector.expiration);
                remaining.insert(sector.sector_number);
                (expirations, remaining)
            },
        );

        // SectorExpirationSets indexed by the expiration height.
        let mut groups = BTreeMap::new();

        // Iterate all declared expirations and try to find the sectors
        for expiration in &declared_expirations {
            let expiration_set = self
                .map
                .get(&expiration)
                .ok_or(ExpirationQueueError::ExpirationSetNotFound)?;

            if let Some(group) = group_expiration_set(&mut all_remaining, expiration_set.clone()) {
                groups.insert(*expiration, group);
            }
        }

        // Traverse expiration sets and try to find the remaining sectors.
        // Remaining sectors should be rescheduled to expire soon, so this
        // traversal should exit early.
        for (expiration, expiration_set) in self.map.iter() {
            // If this set's height is one of our declared expirations, we've
            // already processed it above. Sectors rescheduled to this height
            // would have been included in the earlier processing.
            if declared_expirations.contains(expiration) {
                continue;
            }

            // Check if any of the remaining sectors are in this set.
            if let Some(group) = group_expiration_set(&mut all_remaining, expiration_set.clone()) {
                groups.insert(*expiration, group);
            }

            // All sectors were found
            if all_remaining.is_empty() {
                break;
            }
        }

        // There are still some sectors not found
        if !all_remaining.is_empty() {
            log::error!(target: LOG_TARGET, "find_sectors_by_expiration: Some sectors not found {all_remaining:?}");
            return Err(ExpirationQueueError::SectorNotFound);
        }

        Ok(groups)
    }
}

/// Extract sector numbers from the set if they should be included. None is
/// returned of no sector numbers are found in the current set.
fn group_expiration_set(
    include_set: &mut BTreeSet<SectorNumber>,
    expiration_set: ExpirationSet,
) -> Option<SectorExpirationSet> {
    // Get sector numbers which are in the set that should be included. If any
    // sector is found we remove it from the set.
    let sector_numbers = expiration_set
        .on_time_sectors
        .iter()
        .filter_map(|u| include_set.remove(u).then(|| u).copied())
        .collect::<Vec<_>>();

    // Return `Some` if any sector number from the `expiration_set` was in the
    // `include_set`
    sector_numbers
        .is_empty()
        .not()
        .then(|| SectorExpirationSet {
            sectors: sector_numbers,
            expiration_set,
        })
}

/// Result of the [`ExpirationQueue::find_sectors_by_expiration`] function.
/// Represents a search result.
#[derive(Clone)]
struct SectorExpirationSet {
    /// The sectors we were searching for and are part of the expiration set.
    sectors: Vec<SectorNumber>,
    // Expiration set as found in the expiration queue. This set can be modified
    // and saved back to the [`ExpirationQueue`] when needed.
    expiration_set: ExpirationSet,
}

/// Errors that can occur when interacting with the expiration queue.
#[derive(Decode, Encode, PalletError, TypeInfo, RuntimeDebug)]
pub enum ExpirationQueueError {
    /// Expiration set not found
    ExpirationSetNotFound,
    /// Sector not found in expiration set
    SectorNotFound,
    /// Insertion failed
    InsertionFailed,
}
