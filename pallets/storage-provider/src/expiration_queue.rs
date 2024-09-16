use core::marker::PhantomData;

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
    pub fn add(&mut self, on_time_sectors: &[SectorNumber], early_sectors: &[SectorNumber]) {
        for sector in on_time_sectors {
            self.on_time_sectors.try_insert(*sector);
        }

        for sector in early_sectors {
            self.early_sectors.try_insert(*sector);
        }
    }

    /// Removes sectors from the expiration set in place.
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
        // Self(BoundedBTreeMap::new())
        todo!()
    }

    pub fn add_active_sectors(
        &mut self,
        sectors: &[SectorOnChainInfo<BlockNumber>],
    ) -> Result<(), ExpirationQueueError> {
        todo!()
    }

    pub fn reschedule_expirations(
        &mut self,
        new_expiration: BlockNumber,
        sectors: &[SectorOnChainInfo<BlockNumber>],
    ) -> Result<(), ExpirationQueueError> {
        todo!()
    }

    pub fn reschedule_as_faults(
        new_expiration: BlockNumber,
        sectors: &[SectorOnChainInfo<BlockNumber>],
    ) -> Result<(), ExpirationQueueError> {
        todo!()
    }

    pub fn reschedule_all_as_faults(
        &mut self,
        fault_expiration: BlockNumber,
    ) -> Result<(), ExpirationQueueError> {
        todo!()
    }

    pub fn reschedule_recovered(
        &mut self,
        sectors: &[SectorOnChainInfo<BlockNumber>],
    ) -> Result<(), ExpirationQueueError> {
        todo!()
    }

    pub fn replace_sectors(
        &mut self,
        old_sectors: &[SectorOnChainInfo<BlockNumber>],
        new_sectors: &[SectorOnChainInfo<BlockNumber>],
    ) -> Result<(), ExpirationQueueError> {
        todo!()
    }

    pub fn remove_sectors(&mut self) -> Result<(), ExpirationQueueError> {
        todo!()
    }

    pub fn pop_until(&mut self, until: BlockNumber) -> Result<ExpirationSet, ExpirationQueueError> {
        todo!()
    }
}

#[derive(Decode, Encode, PalletError, TypeInfo, RuntimeDebug)]
pub enum ExpirationQueueError {}
