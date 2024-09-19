extern crate alloc;

use alloc::{collections::BTreeSet, vec::Vec};

use codec::{Decode, Encode};
use frame_support::{pallet_prelude::*, sp_runtime::BoundedBTreeSet, PalletError};
use primitives_proofs::SectorNumber;
use scale_info::TypeInfo;

use crate::{
    expiration_queue::ExpirationQueue,
    sector::{SectorOnChainInfo, MAX_SECTORS},
};

/// Max amount of partitions per deadline.
/// ref: <https://github.com/filecoin-project/builtin-actors/blob/82d02e58f9ef456aeaf2a6c737562ac97b22b244/runtime/src/runtime/policy.rs#L283>
pub const MAX_PARTITIONS_PER_DEADLINE: u32 = 3000;
const LOG_TARGET: &'static str = "runtime::storage_provider::partition";
pub type PartitionNumber = u32;

#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq, TypeInfo)]
pub struct Partition<BlockNumber>
where
    BlockNumber: sp_runtime::traits::BlockNumber,
{
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

    /// All sectors mapped by the expiration. The sectors are indexed by the
    /// expiration block. An expiration may be an "on-time" scheduled
    /// expiration, or early "faulty" expiration.
    pub expirations: ExpirationQueue<BlockNumber>,

    /// Sectors that were terminated before their committed expiration, indexed by termination block.
    pub early_terminations: BoundedBTreeMap<
        BlockNumber,
        BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,
        ConstU32<MAX_SECTORS>,
    >,
}

impl<BlockNumber> Partition<BlockNumber>
where
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    pub fn new() -> Self {
        Self {
            sectors: BoundedBTreeSet::new(),
            unproven: BoundedBTreeSet::new(),
            faults: BoundedBTreeSet::new(),
            recoveries: BoundedBTreeSet::new(),
            terminated: BoundedBTreeSet::new(),
            expirations: ExpirationQueue::new(),
            early_terminations: BoundedBTreeMap::new(),
        }
    }

    /// Live sectors are sectors that are not terminated (i.e. not in `terminated`).
    pub fn live_sectors(&self) -> BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>> {
        self.sectors
            .difference(&self.terminated)
            .copied()
            .collect::<BTreeSet<_>>()
            .try_into()
            .expect("Sectors is bounded to MAX_SECTORS so the length can never exceed MAX_SECTORS")
    }

    /// Adds sectors to this partition.
    /// The sectors are "live", neither faulty, recovering, nor terminated.
    ///
    /// condition: the sector numbers cannot be in any of the `BoundedBTreeSet`'s
    /// fails if any of the given sector numbers are a duplicate
    pub fn add_sectors(
        &mut self,
        sectors: &[SectorOnChainInfo<BlockNumber>],
    ) -> Result<(), PartitionError> {
        // Add sectors to the expirations queue.
        self.expirations.add_active_sectors(sectors).map_err(|_| {
            log::error!(target: LOG_TARGET, "add_sectors: Failed to add sectors to the expirations");
            PartitionError::FailedToAddSector
        })?;

        for sector in sectors {
            // Ensure that the sector number has not been used before.
            // All sector number (including faulty, terminated and unproven) are contained in `sectors` so we only need to check in there.
            ensure!(!self.sectors.contains(&sector.sector_number), {
                log::error!(target: LOG_TARGET, "check_sector_number_duplicate: sector {:?} duplicate in sectors",sector.sector_number);
                PartitionError::DuplicateSectorNumber
            });

            self.sectors
                .try_insert(sector.sector_number)
                .map_err(|_| PartitionError::FailedToAddSector)?;
        }

        Ok(())
    }

    /// Declares a set of sectors faulty. Already faulty sectors are ignored,
    /// terminated sectors are skipped, and recovering sectors are reverted to
    /// faulty.
    /// Returns all sectors marked as faulty (including the previous ones), after the operation.
    /// Filecoin ref: <https://github.com/filecoin-project/builtin-actors/blob/82d02e58f9ef456aeaf2a6c737562ac97b22b244/actors/miner/src/partition_state.rs#L225>
    pub fn record_faults(
        &mut self,
        sectors: &BoundedBTreeMap<
            SectorNumber,
            SectorOnChainInfo<BlockNumber>,
            ConstU32<MAX_SECTORS>,
        >,
        sector_numbers: &BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,
        fault_expiration: BlockNumber,
    ) -> Result<BTreeSet<SectorNumber>, PartitionError>
    where
        BlockNumber: sp_runtime::traits::BlockNumber,
    {
        log::debug!(target: LOG_TARGET, "record_faults: sector_number = {sector_numbers:?}");

        // Split declarations into declarations of new faults, and retraction of declared recoveries.
        // recoveries & sector_numbers
        let retracted_recoveries: BTreeSet<SectorNumber> = self
            .recoveries
            .intersection(&sector_numbers)
            .cloned()
            .collect();
        // sector_numbers - retracted_recoveries
        let new_faults: BTreeSet<SectorNumber> = sector_numbers
            .iter()
            .filter(|sector_number| {
                !retracted_recoveries.contains(sector_number)
                // Ignore any terminated sectors and previously declared or detected faults
                && !self.terminated.contains(&sector_number)
                    && !self.faults.contains(&sector_number)
            })
            .copied()
            .collect();

        log::debug!(target: LOG_TARGET, "record_faults: new_faults = {new_faults:?}, amount = {:?}", new_faults.len());
        let new_fault_sectors: Vec<&SectorOnChainInfo<BlockNumber>> = sectors
            .iter()
            .filter_map(|(sector_number, info)| {
                log::debug!(target: LOG_TARGET, "record_faults: checking sec_num {sector_number}");
                new_faults.contains(&sector_number).then_some(info)
            })
            .collect();

        // Add new faults to state, skip if no new faults.
        if !new_fault_sectors.is_empty() {
            self.add_faults(&new_fault_sectors, fault_expiration)?;
        } else {
            log::debug!(target: LOG_TARGET, "record_faults: No new faults detected");
        }

        // remove faulty recoveries from state, skip if no recoveries set to faulty.
        let retracted_recovery_sectors: BTreeSet<SectorNumber> = sectors
            .iter()
            .filter_map(|(sector_number, _info)| retracted_recoveries.get(&sector_number).copied())
            .collect();

        if !retracted_recovery_sectors.is_empty() {
            self.remove_recoveries(&retracted_recovery_sectors)?;
        } else {
            log::debug!(target: LOG_TARGET, "record_faults: No retracted recoveries detected");
        }

        Ok(new_faults)
    }

    /// marks a set of sectors faulty
    /// References:
    /// * <https://github.com/filecoin-project/builtin-actors/blob/82d02e58f9ef456aeaf2a6c737562ac97b22b244/actors/miner/src/partition_state.rs#L155>
    fn add_faults(
        &mut self,
        sectors: &[&SectorOnChainInfo<BlockNumber>],
        fault_expiration: BlockNumber,
    ) -> Result<(), PartitionError> {
        self.expirations
            .reschedule_as_faults(fault_expiration, sectors).map_err(|_|{
                log::error!(target: LOG_TARGET, "add_faults: Failed to add faults to the expirations");
                PartitionError::FailedToAddFaults
            })?;

        // Update partition metadata
        let sector_numbers = sectors
            .iter()
            .map(|sector| sector.sector_number)
            .collect::<BTreeSet<_>>();
        self.faults = self.faults
            .union(&sector_numbers)
            .cloned()
            .collect::<BTreeSet<_>>()
            .try_into()
            .map_err(|_|{
                log::error!(target: LOG_TARGET, "add_faults: Failed to add sector numbers to faults");
                PartitionError::FailedToAddFaults
            })?;

        log::debug!(target: LOG_TARGET, "add_faults: new faults {:?}", self.faults);

        // Once marked faulty, sectors are moved out of the unproven set.
        for sector_number in sector_numbers {
            self.unproven.remove(&sector_number);
        }
        Ok(())
    }

    /// Removes sectors from recoveries
    fn remove_recoveries(
        &mut self,
        sector_numbers: &BTreeSet<SectorNumber>,
    ) -> Result<(), PartitionError> {
        self.recoveries = self.recoveries.difference(sector_numbers).cloned().collect::<BTreeSet<_>>().try_into().map_err(|_| {
            log::error!(target: LOG_TARGET, "remove_recoveries: Failed to remove sectors from recovering");
            PartitionError::FailedToRemoveRecoveries
        })?;

        Ok(())
    }

    /// Set sectors from faulty to recovering, skips any sectors already marked as non-faulty or recovering
    ///
    /// References:
    /// * <https://github.com/filecoin-project/builtin-actors/blob/0f205c378983ac6a08469b9f400cbb908eef64e2/actors/miner/src/partition_state.rs#L317>
    pub fn declare_faults_recovered(
        &mut self,
        sector_numbers: &BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,
    ) where
        BlockNumber: sp_runtime::traits::BlockNumber,
    {
        // Recoveries = (sector_numbers & self.faults) - self.recoveries
        let new_recoveries = sector_numbers.intersection(&self.faults).copied().collect();
        // self.recoveries | recoveries
        self.recoveries = self
            .recoveries
            .union(&new_recoveries)
            .copied()
            .collect::<BTreeSet<u64>>()
            .try_into()
            .expect("BoundedBTreeSet should be able to be created from BTreeSet");
    }

    /// Removes all previously faulty sectors, declared as recoveries, from faults and clears recoveries.
    ///
    /// References:
    /// * <https://github.com/filecoin-project/builtin-actors/blob/82d02e58f9ef456aeaf2a6c737562ac97b22b244/actors/miner/src/partition_state.rs#L271>
    pub fn recover_all_declared_recoveries(
        &mut self,
        all_sectors: &BoundedBTreeMap<
            SectorNumber,
            SectorOnChainInfo<BlockNumber>,
            ConstU32<MAX_SECTORS>,
        >,
    ) -> Result<(), PartitionError> {
        self.expirations.reschedule_recovered(all_sectors, &self.recoveries).map_err(|err| {
            log::error!(target: LOG_TARGET, "recover_all_declared_recoveries: Failed to reschedule recoveries. error {err:?}");
            PartitionError::FailedToRemoveRecoveries
        })?;

        self.faults = self
            .faults
            .difference(&self.recoveries)
            .copied()
            .collect::<BTreeSet<u64>>()
            .try_into()
            .expect("(faults - recoveries).len() <= faults.len()");

        self.recoveries.clear();

        Ok(())
    }
}

#[derive(Decode, Encode, PalletError, TypeInfo, RuntimeDebug)]
pub enum PartitionError {
    /// Emitted when adding sectors fails
    FailedToAddSector,
    /// Emitted when trying to add a sector number that has already been used in this partition.
    DuplicateSectorNumber,
    /// Emitted when adding faults fails
    FailedToAddFaults,
    /// Emitted when removing recovering sectors fails
    FailedToRemoveRecoveries,
}

#[cfg(test)]
mod test {
    use primitives_proofs::RegisteredSealProof;

    use super::*;

    #[test]
    fn add_sectors() -> Result<(), PartitionError> {
        // Set up partition, using `u64` for block number because it is not relevant to this test.
        let mut partition: Partition<u64> = Partition::new();
        // Add some sectors
        let sectors_to_add = vec![
            SectorOnChainInfo {
                sector_number: 1,
                seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
                sealed_cid: Default::default(),
                activation: Default::default(),
                expiration: 1000,
                unsealed_cid: Default::default(),
            },
            SectorOnChainInfo {
                sector_number: 2,
                seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
                sealed_cid: Default::default(),
                activation: Default::default(),
                expiration: 1100,
                unsealed_cid: Default::default(),
            },
        ];

        // Add sectors to the partition
        partition.add_sectors(&sectors_to_add)?;

        for sector in sectors_to_add {
            // 1. Check that sector is in active sectors
            assert!(partition.sectors.contains(&sector.sector_number));

            // 2. Check that sector will expire
            let expiration = partition.expirations.map.get(&sector.expiration).unwrap();
            assert!(expiration.on_time_sectors.contains(&sector.sector_number));
        }

        Ok(())
    }

    #[test]
    fn live_sectors() -> Result<(), PartitionError> {
        // Set up partition, using `u64` for block number because it is not relevant to this test.
        let mut partition: Partition<u64> = Partition::new();

        // Add some sectors
        partition.add_sectors(&[
            SectorOnChainInfo {
                sector_number: 1,
                seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
                sealed_cid: Default::default(),
                activation: Default::default(),
                expiration: Default::default(),
                unsealed_cid: Default::default(),
            },
            SectorOnChainInfo {
                sector_number: 2,
                seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
                sealed_cid: Default::default(),
                activation: Default::default(),
                expiration: Default::default(),
                unsealed_cid: Default::default(),
            },
        ])?;

        // Terminate a sector that is in the active sectors.
        partition
            .terminated
            .try_insert(1)
            .expect(&format!("Inserting a single element into terminated sectors of a partition, which is a BoundedBTreeMap with length {MAX_SECTORS}, should not fail (1 < {MAX_SECTORS})"));
        let live_sectors = partition.live_sectors();

        // Create expected result.
        let mut expected_live_sectors: BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>> =
            BoundedBTreeSet::new();

        expected_live_sectors
            .try_insert(2)
            .expect(&format!("Inserting a single element into expected_live_sectors, which is a BoundedBTreeMap with length {MAX_SECTORS}, should not fail (1 < {MAX_SECTORS})"));
        assert_eq!(live_sectors, expected_live_sectors);
        Ok(())
    }
}
