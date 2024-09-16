extern crate alloc;
use alloc::{collections::BTreeMap, vec::Vec};

use codec::{Decode, Encode};
use frame_support::PalletError;
use primitives_proofs::SectorNumber;
use scale_info::TypeInfo;
use sp_core::{ConstU32, RuntimeDebug};
use sp_runtime::{BoundedBTreeMap, BoundedBTreeSet};

use crate::sector::{SectorOnChainInfo, MAX_SECTORS};

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

    /// Adds sectors to the expiration set in place.
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

    /// Removes sectors from the expiration set in place.
    pub fn remove(
        &mut self,
        on_time_sectors: &BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,
        early_sectors: &BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,
    ) {
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

#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq, TypeInfo)]
pub struct ExpirationQueue<BlockNumber>
where
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    pub map: BoundedBTreeMap<
        BlockNumber,
        ExpirationSet,
        ConstU32<MAX_SECTORS>, // TODO: What should be the bound?
    >,
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

    /// Reschedules specified sectors to a new expiration block number. The
    /// sectors being rescheduled are assumed to be not faulty, and hence are
    /// re-scheduled for on-time rather than early expiration.
    ///
    /// https://github.com/filecoin-project/builtin-actors/blob/c3c41c5d06fe78c88d4d05eb81b749a6586a5c9f/actors/miner/src/expiration_queue.rs#L206
    pub fn reschedule_expirations(
        &mut self,
        new_expiration: BlockNumber,
        sectors: &[SectorOnChainInfo<BlockNumber>],
    ) -> Result<(), ExpirationQueueError> {
        if sectors.is_empty() {
            return Ok(());
        }

        // Remove sectors from their current expiration entries.
        self.remove_active_sectors(sectors)?;

        // Add sectors to their new expiration entries.
        let sector_numbers = sectors.iter().map(|s| s.sector_number).collect::<Vec<_>>();
        self.add_to_expiration_set(new_expiration, &sector_numbers, &[])?;

        Ok(())
    }

    /// Re-schedules sectors to expire at an early expiration heigh, if they
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
        todo!()
    }

    /// https://github.com/filecoin-project/builtin-actors/blob/c3c41c5d06fe78c88d4d05eb81b749a6586a5c9f/actors/miner/src/expiration_queue.rs#L294
    pub fn reschedule_all_as_faults(
        &mut self,
        new_expiration: BlockNumber,
    ) -> Result<(), ExpirationQueueError> {
        todo!()
    }

    /// https://github.com/filecoin-project/builtin-actors/blob/c3c41c5d06fe78c88d4d05eb81b749a6586a5c9f/actors/miner/src/expiration_queue.rs#L361
    pub fn reschedule_recovered(
        &mut self,
        sectors: &[SectorOnChainInfo<BlockNumber>],
    ) -> Result<(), ExpirationQueueError> {
        todo!()
    }

    /// https://github.com/filecoin-project/builtin-actors/blob/c3c41c5d06fe78c88d4d05eb81b749a6586a5c9f/actors/miner/src/expiration_queue.rs#L441
    pub fn replace_sectors(
        &mut self,
        old_sectors: &[SectorOnChainInfo<BlockNumber>],
        new_sectors: &[SectorOnChainInfo<BlockNumber>],
    ) -> Result<(), ExpirationQueueError> {
        todo!()
    }

    /// https://github.com/filecoin-project/builtin-actors/blob/c3c41c5d06fe78c88d4d05eb81b749a6586a5c9f/actors/miner/src/expiration_queue.rs#L467
    pub fn remove_sectors(&mut self) -> Result<(), ExpirationQueueError> {
        todo!()
    }

    /// https://github.com/filecoin-project/builtin-actors/blob/c3c41c5d06fe78c88d4d05eb81b749a6586a5c9f/actors/miner/src/expiration_queue.rs#L592
    pub fn pop_until(&mut self, until: BlockNumber) -> Result<ExpirationSet, ExpirationQueueError> {
        todo!()
    }

    /// Add sectors to the specific expiration set.
    ///
    /// https://github.com/filecoin-project/builtin-actors/blob/c3c41c5d06fe78c88d4d05eb81b749a6586a5c9f/actors/miner/src/expiration_queue.rs#L626
    pub fn add_to_expiration_set(
        &mut self,
        expiration: BlockNumber,
        on_time_sectors: &[SectorNumber],
        early_sectors: &[SectorNumber],
    ) -> Result<(), ExpirationQueueError> {
        // TODO: Try to update in place
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
    fn remove_active_sectors(
        &mut self,
        sectors: &[SectorOnChainInfo<BlockNumber>],
    ) -> Result<(), ExpirationQueueError> {
        // Group sectors by their expiration, then remove from existing queue
        // entries according to those groups.
        let groups = self.find_sectors_by_expiration(sectors)?;

        for (expiration_height, expiration_set) in groups {
            let mut set = self
                .map
                .get(&expiration_height)
                .ok_or(ExpirationQueueError::ExpirationSetNotFound)?
                .clone();

            // Remove sectors from the set
            set.remove(&expiration_set.on_time_sectors, &BoundedBTreeSet::new());

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
    fn find_sectors_by_expiration(
        &self,
        sectors: &[SectorOnChainInfo<BlockNumber>],
    ) -> Result<BTreeMap<BlockNumber, ExpirationSet>, ExpirationQueueError> {
        todo!()
    }
}

/// Errors that can occur when interacting with the expiration queue.
#[derive(Decode, Encode, PalletError, TypeInfo, RuntimeDebug)]
pub enum ExpirationQueueError {
    /// Expiration set not found
    ExpirationSetNotFound,
    /// Insertion failed
    InsertionFailed,
}
