use core::cmp::Ord;

use codec::{Decode, Encode};
use frame_support::{pallet_prelude::*, sp_runtime::BoundedBTreeSet, PalletError};
use primitives_proofs::SectorNumber;
use scale_info::TypeInfo;

use crate::{pallet::LOG_TARGET, sector::MAX_SECTORS};

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
            self.check_sector_number_duplicate(&sector_number)?;
            self.sectors
                .try_insert(sector_number)
                .map_err(|_| PartitionError::FailedToAddSector)?;
        }
        Ok(())
    }

    /// Checks if the given sector number is used
    fn check_sector_number_duplicate(
        &self,
        sector_number: &SectorNumber,
    ) -> Result<(), PartitionError> {
        // All sector number (including faulty, terminated and unproven) are contained in `sectors` so we only need to check in there.
        ensure!(!self.sectors.contains(sector_number), {
            log::error!(target: LOG_TARGET, "check_sector_number_duplicate: sector_number {sector_number:?} duplicate in sectors");
            PartitionError::DuplicateSectorNumber
        });
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
