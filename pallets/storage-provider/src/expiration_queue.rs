extern crate alloc;
use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use core::ops::Not;

use codec::{Decode, Encode};
use primitives_proofs::SectorNumber;
use scale_info::TypeInfo;
use sp_core::{ConstU32, RuntimeDebug};
use sp_runtime::{BoundedBTreeMap, BoundedBTreeSet};

use crate::{
    error::GeneralPalletError,
    sector::{SectorOnChainInfo, MAX_SECTORS},
};

const LOG_TARGET: &'static str = "runtime::storage_provider::expiration_queue";

/// ExpirationSet is a collection of sector numbers that are expiring, either
/// due to expected "on-time" expiration at the end of their life, or unexpected
/// "early" termination due to being faulty for too long consecutively.
#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq, TypeInfo)]
pub struct ExpirationSet {
    /// Sectors expiring "on time" at the end of their committed life
    pub on_time_sectors: BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,
    /// Sectors expiring "early" due to being faulty for too long
    pub early_sectors: BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,
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
    ) -> Result<(), GeneralPalletError> {
        for sector in on_time_sectors {
            self.on_time_sectors
                .try_insert(*sector)
                .map_err(|_| {
                    log::error!(target: LOG_TARGET, "add: Could not insert sector into on time sectors"); 
                    GeneralPalletError::ExpirationQueueErrorInsertionFailed
                })?;
        }

        for sector in early_sectors {
            self.early_sectors.try_insert(*sector).map_err(|_| {
                log::error!(target: LOG_TARGET, "add: Could not insert sector into early sectors");
                GeneralPalletError::ExpirationQueueErrorInsertionFailed
            })?;
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
    pub fn len(&self) -> usize {
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
    pub map: BoundedBTreeMap<BlockNumber, ExpirationSet, ConstU32<MAX_SECTORS>>,
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
    ) -> Result<(), GeneralPalletError> {
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
    ) -> Result<(), GeneralPalletError> {
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
    ) -> Result<(), GeneralPalletError> {
        // Sectors remaining to be rescheduled.
        let mut remaining = reschedule
            .iter()
            .map(|s| (*s, all_sectors.get(s).expect("sector should exist").clone()))
            .collect::<BTreeMap<_, _>>();

        // All sectors rescheduled
        let mut sectors_rescheduled = Vec::new();

        // Traverse the expiration queue once to find each recovering sector and
        // remove it from early/faulty.
        for (_, expiration_set) in self.map.iter_mut() {
            // Sectors that were rescheduled from this set
            let mut early_unset = Vec::new();

            // In some cases the sector can be faulty at the end of its normal
            // lifetime. In those cases the early expiration would be after
            // on_time expiration. If that happens the on_time is used as the
            // expiration height. The loop below removes those cases from the
            // `remaining``.
            for sector_number in expiration_set.on_time_sectors.iter() {
                remaining.remove(&sector_number);
            }

            // Remove the remaining sectors from the early set
            for sector_number in expiration_set.early_sectors.iter() {
                let sector = match remaining.remove(&sector_number) {
                    Some(s) => s,
                    None => continue,
                };

                early_unset.push(*sector_number);
                sectors_rescheduled.push(sector);
            }

            // Remove early expiration sectors from this set
            expiration_set.remove(&[], &early_unset);

            // Break when we rescheduled all early sectors
            if remaining.is_empty() {
                break;
            }
        }

        // Remove sets that were emptied from the queue
        self.map
            .retain(|_, expiration_set| !expiration_set.is_empty());

        // There are still some sectors not found
        if !remaining.is_empty() {
            log::error!(target: LOG_TARGET, "reschedule_recovered: Some sectors not found {remaining:?}");
            return Err(GeneralPalletError::ExpirationQueueErrorSectorNotFound);
        }

        // Re-schedule the removed sectors to their target expiration.
        self.add_active_sectors(&sectors_rescheduled)?;

        Ok(())
    }

    /// Remove some sectors from the queue. The sectors may be active or faulty,
    /// and scheduled either for on-time or early termination.
    /// Fails if any sectors are not found in the queue.
    /// In Filecoin this function takes in recoveries. We do not need these as they are only used for power recovery.
    pub fn remove_sectors(
        &mut self,
        sectors: &[SectorOnChainInfo<BlockNumber>],
        faults: &BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,
    ) -> Result<ExpirationSet, GeneralPalletError> {
        let mut remaining: BTreeSet<SectorNumber> =
            sectors.iter().map(|s| s.sector_number).collect();
        let mut removed = ExpirationSet::new();

        // Split into faulty and non-faulty. We process non-faulty sectors first
        // because they always expire on-time so we know where to find them.
        let mut non_faulty_sectors = Vec::<&SectorOnChainInfo<BlockNumber>>::new();
        let mut faulty_sectors = Vec::<&SectorOnChainInfo<BlockNumber>>::new();

        sectors.iter().for_each(|sector| {
            if faults.contains(&sector.sector_number) {
                faulty_sectors.push(sector);
            } else {
                non_faulty_sectors.push(sector);

                // remove them from "remaining", we're going to process them below.
                remaining.remove(&sector.sector_number);
            }
        });

        let removed_sector_numbers = self.remove_active_sectors(&non_faulty_sectors)?;
        removed.on_time_sectors = BoundedBTreeSet::try_from(removed_sector_numbers).expect(
            "sectors may not exceed MAX_SECTORS. This error should not occur with the set bounds",
        );

        // Finally, remove faulty sectors (on time and not).
        self.map.iter_mut().try_for_each(
            |(_block, expiration_set)| -> Result<(), GeneralPalletError> {
                for sector in &faulty_sectors {
                    let sector_number = sector.sector_number;
                    let mut found = false;

                    if expiration_set.on_time_sectors.contains(&sector_number) {
                        found = true;
                        expiration_set.on_time_sectors.remove(&sector_number);
                        removed
                            .on_time_sectors
                            .try_insert(sector_number)
                            .expect("sectors may not exceed MAX_SECTORS. This error should not occur with the set bounds");
                    } else if expiration_set.early_sectors.contains(&sector_number) {
                        found = true;
                        expiration_set.early_sectors.remove(&sector_number);
                        removed
                            .early_sectors
                            .try_insert(sector_number)
                            .expect("sectors may not exceed MAX_SECTORS. This error should not occur with the set bounds");
                    }

                    if found {
                        remaining.remove(&sector_number);
                    }
                }

                Ok(())
            },
        )?;

        if !remaining.is_empty() {
            log::error!(target: LOG_TARGET, "reschedule_recovered: Some sectors not found {remaining:?}");
            return Err(GeneralPalletError::ExpirationQueueErrorSectorNotFound);
        } else {
            Ok(removed)
        }
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
    ) -> Result<(), GeneralPalletError> {
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
            .map_err(|_| {
                log::error!(target: LOG_TARGET, "add_to_expiration: Could not insert expiration set into queue");
                GeneralPalletError::ExpirationQueueErrorInsertionFailed
            })?;

        Ok(())
    }

    /// Removes active sectors from the queue.
    ///
    /// https://github.com/filecoin-project/builtin-actors/blob/c3c41c5d06fe78c88d4d05eb81b749a6586a5c9f/actors/miner/src/expiration_queue.rs#L672
    fn remove_active_sectors(
        &mut self,
        sectors: &[&SectorOnChainInfo<BlockNumber>],
    ) -> Result<BTreeSet<SectorNumber>, GeneralPalletError> {
        let mut removed_sector_numbers = BTreeSet::new();
        // Group sectors by their expiration, then remove from existing queue
        // entries according to those groups.
        let groups = self.find_sectors_by_expiration(sectors)?;

        for (expiration_height, set) in groups {
            let mut expiration_set = self
                .map
                .get(&expiration_height)
                .ok_or(GeneralPalletError::ExpirationQueueErrorExpirationSetNotFound)?
                .clone();

            // Remove sectors from the set
            expiration_set.remove(&set.sectors, &[]);

            // Update the expiration set
            self.must_update_or_delete(expiration_height, expiration_set)?;

            removed_sector_numbers.extend(&set.sectors);
        }

        Ok(removed_sector_numbers)
    }

    /// Updates the expiration set for a given expiration block number, or
    /// removes it if the set becomes empty.
    fn must_update_or_delete(
        &mut self,
        expiration: BlockNumber,
        expiration_set: ExpirationSet,
    ) -> Result<(), GeneralPalletError> {
        if expiration_set.is_empty() {
            self.map.remove(&expiration);
        } else {
            self.map
                .try_insert(expiration, expiration_set)
                .map_err(|_| {
                    log::error!(target: LOG_TARGET, "add_to_expiration: Could not insert expiration set into queue");
                    GeneralPalletError::ExpirationQueueErrorInsertionFailed
                })?;
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
    ) -> Result<BTreeMap<BlockNumber, SectorExpirationSet>, GeneralPalletError> {
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
                .ok_or(GeneralPalletError::ExpirationQueueErrorExpirationSetNotFound)?;

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
            return Err(GeneralPalletError::ExpirationQueueErrorSectorNotFound);
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

#[cfg(test)]
mod tests {
    extern crate alloc;

    use alloc::collections::btree_set::BTreeSet;

    use primitives_proofs::SectorNumber;
    use sp_runtime::BoundedBTreeSet;

    use crate::{expiration_queue::ExpirationQueue, sector::SectorOnChainInfo};

    #[test]
    fn remove_sectors() {
        let mut q = ExpirationQueue::new();
        // Add sectors to queue
        q.add_active_sectors(&sectors()).unwrap();

        // put queue in a state where some sectors are early and some are faulty
        q.reschedule_as_faults(
            8, // run to block 8 so sector 1 and 4 are on time and sector 5 and 6 are early
            &sectors()[1..]
                .iter()
                .collect::<Vec<&SectorOnChainInfo<u64>>>(),
        )
        .unwrap();

        // remove an active sector from first set, faulty sector and early faulty sector from second set,
        let to_remove = [
            sectors()[0].clone(),
            sectors()[3].clone(),
            sectors()[4].clone(),
            sectors()[5].clone(),
        ];

        // and only sector from last set
        let faults = BoundedBTreeSet::try_from(BTreeSet::from([4, 5, 6])).unwrap();

        let result = q.remove_sectors(&to_remove, &faults);
        assert!(result.is_ok());
        let removed = result.unwrap();
        let expected_on_time_sectors = BTreeSet::from([1, 4]);
        let expected_early_sectors = BTreeSet::from([5, 6]);

        // assert all return values are correct
        assert_eq!(
            removed.on_time_sectors.into_inner(),
            expected_on_time_sectors
        );
        assert_eq!(removed.early_sectors.into_inner(), expected_early_sectors);
    }

    fn sectors() -> [SectorOnChainInfo<u64>; 6] {
        [
            test_sector(2, 1),
            test_sector(3, 2),
            test_sector(7, 3),
            test_sector(8, 4),
            test_sector(11, 5),
            test_sector(13, 6),
        ]
    }

    fn test_sector(expiration: u64, sector_number: SectorNumber) -> SectorOnChainInfo<u64> {
        SectorOnChainInfo {
            expiration,
            sector_number,
            ..Default::default()
        }
    }
}
