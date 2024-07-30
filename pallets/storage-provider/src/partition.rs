extern crate alloc;

use alloc::{collections::BTreeSet, vec::Vec};

use core::cmp::Ord;

use codec::{Decode, Encode};
use frame_support::{pallet_prelude::*, sp_runtime::BoundedBTreeSet, PalletError};
use primitives_proofs::SectorNumber;
use scale_info::TypeInfo;

use crate::{
    pallet::LOG_TARGET,
    sector::{SectorOnChainInfo, MAX_SECTORS},
};

/// Max amount of partitions per deadline.
/// ref: <https://github.com/filecoin-project/builtin-actors/blob/82d02e58f9ef456aeaf2a6c737562ac97b22b244/runtime/src/runtime/policy.rs#L283>
pub const MAX_PARTITIONS_PER_DEADLINE: u32 = 3000;
pub type PartitionNumber = u32;

#[derive(Clone, Debug, Decode, Encode, PartialEq, TypeInfo)]
pub struct Partition<BlockNumber> {
    /// All sector numbers in this partition, including faulty, unproven and terminated sectors.
    pub sectors: BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,

    /// Unproven sectors in this partition. This will be cleared on
    /// a successful window post (or at the end of the partition's next
    /// deadline). At that time, any still unproven sectors will be added to
    /// the faulty sectors.
    pub unproven: BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,

    /// Subset of sectors detected/declared faulty and not yet recovered (excl. from PoSt).
    /// The intersection of `faults` and `terminated` is always empty.
    ///
    /// Used in the `declare_faults` extrinsic
    /// TODO: Add helper method for adding faults.
    pub faults: BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,

    /// Subset of faulty sectors expected to recover on next PoSt
    /// The intersection of `recoveries` and `terminated` is always empty.
    ///
    /// Used in the `declare_faults_recovered` extrinsic
    /// TODO: Add helper method for adding recoveries.
    pub recoveries: BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,

    /// Subset of sectors terminated but not yet removed from partition (excl. from PoSt)
    /// TODO: Add helper method for adding terminated sectors.
    pub terminated: BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,

    /// Sectors that were terminated before their committed expiration, indexed by termination block.
    pub early_terminations: BoundedBTreeMap<
        BlockNumber,
        BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,
        ConstU32<MAX_SECTORS>,
    >,
}

impl<BlockNumber> Partition<BlockNumber>
where
    BlockNumber: Ord,
{
    pub fn new() -> Self {
        Self {
            sectors: BoundedBTreeSet::new(),
            unproven: BoundedBTreeSet::new(),
            faults: BoundedBTreeSet::new(),
            recoveries: BoundedBTreeSet::new(),
            terminated: BoundedBTreeSet::new(),
            early_terminations: BoundedBTreeMap::new(),
        }
    }

    /// Live sectors are sectors that are not terminated (i.e. not in `terminated` or `early_terminations`).
    pub fn live_sectors(
        &self,
    ) -> Result<BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>, PartitionError> {
        let mut live_sectors = BoundedBTreeSet::new();
        let difference = self.sectors.difference(&self.terminated).cloned();
        for sector_number in difference {
            live_sectors
                .try_insert(sector_number)
                .map_err(|_| PartitionError::FailedToGetLiveSectors)?;
        }
        Ok(live_sectors)
    }

    /// Adds sectors to this partition.
    /// The sectors are "live", neither faulty, recovering, nor terminated.
    ///
    /// condition: the sector numbers cannot be in any of the `BoundedBTreeSet`'s
    /// fails if any of the given sector numbers are a duplicate
    pub fn add_sectors(&mut self, sectors: &[SectorNumber]) -> Result<(), PartitionError> {
        let new_sectors = sectors.iter().cloned();
        for sector_number in new_sectors {
            // Ensure that the sector number has not been used before.
            // All sector number (including faulty, terminated and unproven) are contained in `sectors` so we only need to check in there.
            ensure!(!self.sectors.contains(&sector_number), {
                log::error!(target: LOG_TARGET, "check_sector_number_duplicate: sector_number {sector_number:?} duplicate in sectors");
                PartitionError::DuplicateSectorNumber
            });
            self.sectors
                .try_insert(sector_number)
                .map_err(|_| PartitionError::FailedToAddSector)?;
        }
        Ok(())
    }

    /// Declares a set of sectors faulty. Already faulty sectors are ignored,
    /// terminated sectors are skipped, and recovering sectors are reverted to
    /// faulty.
    /// Filecoin ref: <https://github.com/filecoin-project/builtin-actors/blob/82d02e58f9ef456aeaf2a6c737562ac97b22b244/actors/miner/src/partition_state.rs#L225>
    pub fn record_faults(
        &mut self,
        sectors: &BoundedBTreeMap<
            SectorNumber,
            SectorOnChainInfo<BlockNumber>,
            ConstU32<MAX_SECTORS>,
        >,
        sector_numbers: &BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,
    ) -> Result<BTreeSet<SectorNumber>, PartitionError>
    where
        BlockNumber: sp_runtime::traits::BlockNumber,
    {
        // Split declarations into declarations of new faults, and retraction of declared recoveries.
        // recoveries & sector_numbers
        let retracted_recoveries: BTreeSet<SectorNumber> = self
            .recoveries
            .intersection(sector_numbers)
            .cloned()
            .collect();
        // sector_numbers - retracted_recoveries
        let mut new_faults: BTreeSet<SectorNumber> = sector_numbers
            .difference(&retracted_recoveries)
            .cloned()
            .collect();
        // Ignore any terminated sectors and previously declared or detected faults
        new_faults = new_faults.difference(&self.terminated).cloned().collect();
        new_faults = new_faults.difference(&self.faults).cloned().collect();

        let new_fault_sectors: Vec<(&SectorNumber, &SectorOnChainInfo<BlockNumber>)> = sectors
            .iter()
            .filter(|(sector_number, _info)| new_faults.contains(&sector_number))
            .collect();
        // Add new faults to state, skip if no new faults.
        if !new_fault_sectors.is_empty() {
            self.add_faults()?;
        }
        // remove faulty recoveries from state, skip if no recoveries set to faulty.
        let retracted_recovery_sectors: BTreeSet<SectorNumber> = sectors
            .iter()
            .filter_map(|(sector_number, _info)| {
                if sector_numbers.contains(&sector_number) {
                    Some(*sector_number)
                } else {
                    None
                }
            })
            .collect();
        if !retracted_recovery_sectors.is_empty() {
            self.remove_recoveries(&retracted_recovery_sectors)?;
        }
        Ok(new_faults)
    }

    /// marks a set of sectors faulty
    pub fn add_faults(&mut self) -> Result<(), PartitionError> {
        // Load expiration queue
        // let mut queue = ExpirationQueue::new(store, &self.expirations_epochs, quant)
        //     .map_err(|e| e.downcast_wrap("failed to load partition queue"))?;

        // Reschedule faults
        // queue
        //     .reschedule_as_faults(fault_expiration, sectors, sector_size)
        //     .map_err(|e| e.downcast_wrap("failed to add faults to partition queue"))?;

        // Save expiration queue
        // self.expirations_epochs = queue.amt.flush()?;

        // Update partition metadata
        // self.faults |= sector_numbers;

        // The sectors must not have been previously faulty or recovering.
        // No change to recoveries or terminations.
        // self.faulty_power += &new_faulty_power;

        // Once marked faulty, sectors are moved out of the unproven set.
        // let unproven = sector_numbers & &self.unproven;

        // self.unproven -= &unproven;
        Ok(())
    }

    /// Removes sectors from recoveries
    pub fn remove_recoveries(
        &mut self,
        sector_numbers: &BTreeSet<SectorNumber>,
    ) -> Result<(), PartitionError> {
        if sector_numbers.is_empty() {
            return Ok(());
        }

        // TODO(catch error)
        let recoveries: BTreeSet<SectorNumber> = self
            .recoveries
            .difference(sector_numbers)
            .cloned()
            .collect();
        self.recoveries = recoveries
            .try_into()
            .map_err(|_| PartitionError::FailedToRemoveRecoveries)?;
        Ok(())
    }
}

#[derive(Decode, Encode, PalletError, TypeInfo, RuntimeDebug)]
pub enum PartitionError {
    /// Emitted when trying to get the live sectors for a partition fails.
    FailedToGetLiveSectors,
    /// Emitted when adding sectors fails
    FailedToAddSector,
    /// Emitted when trying to add a sector number that has already been used in this partition.
    DuplicateSectorNumber,
    /// Emitted when removing recovering sectors fails
    FailedToRemoveRecoveries,
}

#[cfg(test)]
mod test {
    use frame_support::sp_runtime::bounded_vec;

    use super::*;

    #[test]
    fn add_sectors() -> Result<(), PartitionError> {
        // Set up partition, using `u64` for block number because it is not relevant to this test.
        let mut partition: Partition<u64> = Partition::new();
        // Add some sectors
        let sectors_to_add: BoundedVec<SectorNumber, ConstU32<MAX_SECTORS>> = bounded_vec![1, 2];
        partition.add_sectors(&sectors_to_add)?;
        for sector_number in sectors_to_add {
            assert!(partition.sectors.contains(&sector_number));
        }
        Ok(())
    }

    #[test]
    fn live_sectors() -> Result<(), PartitionError> {
        // Set up partition, using `u64` for block number because it is not relevant to this test.
        let mut partition: Partition<u64> = Partition::new();
        // Add some sectors
        partition.add_sectors(&[1, 2])?;
        // Terminate a sector that is in the active sectors.
        partition
            .terminated
            .try_insert(1)
            .expect("Programmer error");
        let live_sectors = partition.live_sectors()?;
        // Create expected result.
        let mut expected_live_sectors: BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>> =
            BoundedBTreeSet::new();
        expected_live_sectors
            .try_insert(2)
            .expect("Programmer error");
        assert_eq!(live_sectors, expected_live_sectors);
        Ok(())
    }
}
