extern crate alloc;

use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use core::ops::AddAssign;

use codec::{Decode, Encode};
use frame_support::{pallet_prelude::*, sp_runtime::BoundedBTreeSet};
use primitives_proofs::SectorNumber;
use scale_info::TypeInfo;

use crate::{
    error::GeneralPalletError,
    expiration_queue::{ExpirationQueue, ExpirationSet},
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
    ) -> Result<(), GeneralPalletError> {
        // Add sectors to the expirations queue.
        self.expirations.add_active_sectors(sectors)?;

        for sector in sectors {
            // Ensure that the sector number has not been used before.
            // All sector number (including faulty, terminated and unproven) are contained in `sectors` so we only need to check in there.
            ensure!(!self.sectors.contains(&sector.sector_number), {
                log::error!(target: LOG_TARGET, "check_sector_number_duplicate: sector {:?} duplicate in sectors",sector.sector_number);
                GeneralPalletError::PartitionErrorDuplicateSectorNumber
            });

            self.sectors.try_insert(sector.sector_number).map_err(|_| {
                log::error!(target: LOG_TARGET, "add_sectors: Failed to add sectors");
                GeneralPalletError::PartitionErrorFailedToAddSector
            })?;
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
    ) -> Result<BTreeSet<SectorNumber>, GeneralPalletError>
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
    ) -> Result<(), GeneralPalletError> {
        self.expirations
            .reschedule_as_faults(fault_expiration, sectors).map_err(|e| {
                log::error!(target: LOG_TARGET, e:?; "add_faults: Failed to add faults to the expirations");
                GeneralPalletError::PartitionErrorFailedToAddFaults
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
                GeneralPalletError::PartitionErrorFailedToAddFaults
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
    ) -> Result<(), GeneralPalletError> {
        self.recoveries = self.recoveries.difference(sector_numbers).cloned().collect::<BTreeSet<_>>().try_into().map_err(|_| {
            log::error!(target: LOG_TARGET, "remove_recoveries: Failed to remove sectors from recovering");
            GeneralPalletError::PartitionErrorFailedToRemoveRecoveries
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
    ) -> Result<(), GeneralPalletError> {
        self.expirations.reschedule_recovered(all_sectors, &self.recoveries).map_err(|err| {
            log::error!(target: LOG_TARGET, "recover_all_declared_recoveries: Failed to reschedule recoveries. error {err:?}");
            GeneralPalletError::PartitionErrorFailedToRemoveRecoveries
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

    /// Marks a collection of sectors as terminated.
    /// The sectors are removed from Faults and Recoveries.
    ///
    ///  Reference implementation:
    /// * <https://github.com/filecoin-project/builtin-actors/blob/8d957d2901c0f2044417c268f0511324f591cb92/actors/miner/src/partition_state.rs#L480>
    pub fn terminate_sectors(
        &mut self,
        block_number: BlockNumber,
        sectors: &[SectorOnChainInfo<BlockNumber>],
    ) -> Result<ExpirationSet, GeneralPalletError> {
        let sector_numbers: BTreeSet<_> = sectors.iter().map(|s| s.sector_number).collect();
        // Ensure that all given sectors are live
        ensure!(
            sector_numbers
                .difference(&self.live_sectors())
                .next()
                .is_none(),
            {
                log::error!(target: LOG_TARGET, "terminate_sectors: can only terminate live sectors");
                GeneralPalletError::PartitionErrorSectorsNotLive
            }
        );

        let removed = self.expirations.remove_sectors(sectors, &self.faults)?;
        // Since the `on_time_sectors` in the `ExpirationSet` are bounded to `MAX_SECTORS` this conversion will never be > MAX_SECTORS.
        let removed_sectors = removed
            .on_time_sectors
            .union(&removed.early_sectors)
            .copied()
            .collect::<BTreeSet<_>>()
            .try_into()
            .expect("Conversion to a set bounded at MAX_SECTORS should always be possible");

        // Record early terminations
        self.record_early_terminations(block_number, &removed_sectors)?;
        let unproven_sectors = removed_sectors
            .intersection(&self.unproven)
            .copied()
            .collect::<BTreeSet<_>>();

        // Update partition metadata
        // All these conversion are unwrapped with expect because they are being created from a subset of sets bounded by MAX_SECTORS.
        self.faults = self
            .faults
            .difference(&removed_sectors)
            .copied()
            .collect::<BTreeSet<_>>()
            .try_into()
            .expect("Conversion to a set bounded at MAX_SECTORS should always be possible");
        self.recoveries = self
            .recoveries
            .difference(&removed_sectors)
            .copied()
            .collect::<BTreeSet<_>>()
            .try_into()
            .expect("Conversion to a set bounded at MAX_SECTORS should always be possible");
        self.terminated = self
            .terminated
            .union(&removed_sectors)
            .copied()
            .collect::<BTreeSet<_>>()
            .try_into()
            .expect("Conversion to a set bounded at MAX_SECTORS should always be possible");
        self.unproven = self
            .unproven
            .difference(&unproven_sectors)
            .copied()
            .collect::<BTreeSet<_>>()
            .try_into()
            .expect("Conversion to a set bounded at MAX_SECTORS should always be possible");
        Ok(removed)
    }

    pub fn record_early_terminations(
        &mut self,
        block_number: BlockNumber,
        sectors: &BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,
    ) -> Result<(), GeneralPalletError> {
        self.early_terminations
            .try_insert(block_number, sectors.clone())
            .expect("Reached the limit for early terminations");
        Ok(())
    }
    /// Pops early terminations until `max_sectors` or until there are none left
    ///
    /// Reference implementation:
    /// * <https://github.com/filecoin-project/builtin-actors/blob/8d957d2901c0f2044417c268f0511324f591cb92/actors/miner/src/partition_state.rs#L640>
    pub fn pop_early_terminations(
        &mut self,
        max_sectors: u64,
    ) -> Result<(TerminationResult<BlockNumber>, /* has more */ bool), GeneralPalletError> {
        let mut processed = Vec::new();
        let mut remaining = None;
        let mut result = TerminationResult::new();
        result.partitions_processed = 1;

        for (&block_number, sectors) in &self.early_terminations {
            let count = sectors.len() as u64;
            let limit = max_sectors - result.sectors_processed;

            let to_process = if limit < count {
                // Filter out sector number that are < limit
                let to_process = sectors
                    .iter()
                    .take(limit as usize)
                    .copied()
                    .collect::<BTreeSet<_>>();
                // Filter out the rest of the sectors that are not processed.
                // Remaining is sectors - to_process
                let rest = sectors
                    .iter()
                    .copied()
                    .filter(|sector_number| !to_process.contains(&sector_number))
                    .collect::<BTreeSet<_>>();
                remaining = Some((rest, block_number));
                result.sectors_processed += limit;
                to_process
            } else {
                processed.push(block_number);
                result.sectors_processed += count;
                sectors.clone().into_inner()
            };

            result.sectors.insert(block_number, to_process);

            if result.sectors_processed >= max_sectors {
                break;
            }
        }

        if let Some((remaining_sectors, remaining_block)) = remaining {
            self.early_terminations
                .try_insert(
                    remaining_block,
                    remaining_sectors
                        .try_into()
                        .expect("Cannot convert remaining sectors"),
                )
                .expect("Failed to add remaining sectors to early terminations");
        }

        // Update early terminations
        self.early_terminations = self
            .early_terminations
            .iter()
            .filter_map(|(block_number, sectors)| {
                (!processed.contains(block_number)).then(|| (*block_number, sectors.clone()))
            })
            .collect::<BTreeMap<_, _>>()
            .try_into()
            .expect("Failed to remove entries from early terminations");

        let has_more = self.early_terminations.iter().next().is_some();
        Ok((result, has_more))
    }
}

pub struct TerminationResult<BlockNumber> {
    /// Sectors maps block numbers at which sectors expired, to sector numbers.
    pub sectors: BTreeMap<BlockNumber, BTreeSet<SectorNumber>>,
    pub partitions_processed: u64,
    pub sectors_processed: u64,
}

impl<BlockNumber> AddAssign for TerminationResult<BlockNumber>
where
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    fn add_assign(&mut self, rhs: Self) {
        self.partitions_processed += rhs.partitions_processed;
        self.sectors_processed += rhs.sectors_processed;

        for (block_number, new_sectors) in rhs.sectors.into_iter() {
            self.sectors
                .entry(block_number)
                .and_modify(|sectors| *sectors = sectors.union(&new_sectors).copied().collect())
                .or_insert(new_sectors);
        }
    }
}

impl<BlockNumber> TerminationResult<BlockNumber>
where
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    pub fn new() -> Self {
        Self {
            sectors: BTreeMap::new(),
            partitions_processed: 0,
            sectors_processed: 0,
        }
    }

    /// Returns true if we're below the partition/sector limit. Returns false if
    /// we're at (or above) the limit.
    pub fn below_limit(&self, partition_limit: u64, sector_limit: u64) -> bool {
        self.partitions_processed < partition_limit && self.sectors_processed < sector_limit
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use alloc::collections::BTreeMap;

    use super::*;

    fn sectors() -> Vec<SectorOnChainInfo<u64>> {
        vec![
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

    #[test]
    fn add_sectors() -> Result<(), GeneralPalletError> {
        // Set up partition, using `u64` for block number because it is not relevant to this test.
        let mut partition: Partition<u64> = Partition::new();
        // Add some sectors
        let sectors_to_add = sectors();

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
    fn live_sectors() -> Result<(), GeneralPalletError> {
        // Set up partition, using `u64` for block number because it is not relevant to this test.
        let mut partition: Partition<u64> = Partition::new();

        let sectors_to_add = sectors();
        // Add some sectors
        partition.add_sectors(&sectors_to_add)?;

        // Terminate a sector that is in the active sectors.
        partition
            .terminated
            .try_insert(sectors_to_add[0].sector_number)
            .expect(&format!("Inserting a single element into terminated sectors of a partition, which is a BoundedBTreeMap with length {MAX_SECTORS}, should not fail (1 < {MAX_SECTORS})"));
        let live_sectors = partition.live_sectors();

        // Create expected result.
        let mut expected_live_sectors: BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>> =
            BoundedBTreeSet::try_from(
                sectors_to_add
                    .iter()
                    .filter_map(|s| {
                        if s.sector_number != 1 {
                            Some(s.sector_number)
                        } else {
                            None
                        }
                    })
                    .collect::<BTreeSet<_>>(),
            )
            .unwrap();

        expected_live_sectors
            .try_insert(2)
            .expect(&format!("Inserting a single element into expected_live_sectors, which is a BoundedBTreeMap with length {MAX_SECTORS}, should not fail (1 < {MAX_SECTORS})"));
        assert_eq!(live_sectors, expected_live_sectors);
        Ok(())
    }

    #[test]
    fn terminate_sectors() -> Result<(), GeneralPalletError> {
        // Set up partition, using `u64` for block number because it is not relevant to this test.
        let mut partition: Partition<u64> = Partition::new();
        let all_sectors = sectors();

        // Add sectors
        partition.add_sectors(&all_sectors)?;

        let all_sectors_map = BoundedBTreeMap::try_from(
            all_sectors
                .iter()
                .map(|s| (s.sector_number, s.clone()))
                .collect::<BTreeMap<SectorNumber, SectorOnChainInfo<u64>>>(),
        )
        .unwrap();
        // fault sector 3, 4, 5 and 6
        let faults = BoundedBTreeSet::try_from(BTreeSet::from([3, 4, 5, 6])).unwrap();
        partition.record_faults(&all_sectors_map, &faults, 7)?;

        // mark 4 and 5 as a recoveries
        let recoveries = BoundedBTreeSet::try_from(BTreeSet::from([4, 5])).unwrap();
        partition.declare_faults_recovered(&recoveries);

        // now terminate 1, 3, 5, and 6
        let terminations = [
            all_sectors[0].clone(), // on time
            all_sectors[2].clone(), // on time
            all_sectors[4].clone(), // early
            all_sectors[5].clone(), // early
        ];
        let termination_block = 3;
        let removed = partition.terminate_sectors(termination_block, &terminations)?;

        let expected_terminations: BTreeSet<_> =
            terminations.iter().map(|s| s.sector_number).collect();
        let expected_sectors: BTreeSet<_> = all_sectors.iter().map(|s| s.sector_number).collect();

        // Assert that the returned expiration set is as expected
        assert_eq!(removed.on_time_sectors.into_inner(), BTreeSet::from([1, 3]));
        assert_eq!(removed.early_sectors.into_inner(), BTreeSet::from([5, 6]));

        // Assert the partition metadata is as expected
        assert_eq!(partition.faults.into_inner(), BTreeSet::from([4]));
        assert_eq!(partition.recoveries.into_inner(), BTreeSet::from([4]));
        assert_eq!(partition.terminated.into_inner(), expected_terminations);
        assert_eq!(partition.sectors.into_inner(), expected_sectors);
        assert_eq!(partition.unproven.into_inner(), BTreeSet::new());

        Ok(())
    }

    #[test]
    fn terminate_sectors_fail_sector_not_live() -> Result<(), GeneralPalletError> {
        // Set up partition, using `u64` for block number because it is not relevant to this test.
        let mut partition: Partition<u64> = Partition::new();

        // Terminate a sector that is not live
        let result = partition.terminate_sectors(1, &[test_sector(1, 6)]);

        assert!(matches!(
            result,
            Err(GeneralPalletError::PartitionErrorSectorsNotLive)
        ));
        Ok(())
    }

    #[test]
    fn pop_early_terminations_till_max_sectors() -> Result<(), GeneralPalletError> {
        // Set up partition, using `u64` for block number because it is not relevant to this test.
        let max_sectors = 1;
        let mut partition: Partition<u64> = Partition::new();
        let sectors = sectors();
        let sector_map = sectors
            .iter()
            .map(|s| (s.sector_number, s.clone()))
            .collect::<BTreeMap<_, _>>()
            .try_into()
            .unwrap();

        // Add sectors to the partition
        partition.add_sectors(&sectors)?;

        // fault sector 3, 4, 5 and 6
        let fault_set = BTreeSet::from([3, 4, 5, 6]).try_into().unwrap();
        partition.record_faults(&sector_map, &fault_set, 7)?;

        // now terminate 1, 3 and 5
        let terminations = [sectors[0].clone(), sectors[2].clone(), sectors[4].clone()];
        let termination_block = 3;
        partition.terminate_sectors(termination_block, &terminations)?;

        // pop first termination
        let (result, has_more) = partition.pop_early_terminations(max_sectors)?;

        assert!(has_more);
        assert_eq!(result.sectors_processed, 1);
        assert_eq!(result.partitions_processed, 1);
        let terminated_sector = result.sectors.get(&termination_block);
        assert!(terminated_sector.is_some());
        let terminated_sector = terminated_sector.unwrap();
        assert_eq!(terminated_sector, &BTreeSet::from([1]));
        let early_termination_sectors = partition
            .early_terminations
            .get(&termination_block)
            .unwrap();
        assert_eq!(early_termination_sectors.len(), 2);

        // pop the next one
        let (result, has_more) = partition.pop_early_terminations(max_sectors).unwrap();

        // expect 3
        assert!(has_more);
        assert_eq!(result.sectors_processed, 1);
        assert_eq!(result.partitions_processed, 1);
        let terminated_sector = result.sectors.get(&termination_block);
        assert!(terminated_sector.is_some());
        let terminated_sector = terminated_sector.unwrap();
        assert_eq!(terminated_sector, &BTreeSet::from([3]));
        let early_termination_sectors = partition
            .early_terminations
            .get(&termination_block)
            .unwrap();
        assert_eq!(early_termination_sectors.len(), 1);

        // Finally pop the last one
        let (result, has_more) = partition.pop_early_terminations(max_sectors).unwrap();

        // expect 5
        assert!(!has_more);
        assert_eq!(result.sectors_processed, 1);
        assert_eq!(result.partitions_processed, 1);
        let terminated_sector = result.sectors.get(&termination_block);
        assert!(terminated_sector.is_some());
        let terminated_sector = terminated_sector.unwrap();
        assert_eq!(terminated_sector, &BTreeSet::from([5]));

        // expect early terminations to be empty
        assert!(partition.early_terminations.is_empty());
        Ok(())
    }

    #[test]
    fn pop_early_terminations() -> Result<(), GeneralPalletError> {
        // Set up partition, using `u64` for block number because it is not relevant to this test.
        let mut partition: Partition<u64> = Partition::new();
        let sectors = sectors();
        let sector_map = sectors
            .iter()
            .map(|s| (s.sector_number, s.clone()))
            .collect::<BTreeMap<_, _>>()
            .try_into()
            .unwrap();

        // Add sectors to the partition
        partition.add_sectors(&sectors)?;

        // fault sector 3, 4, 5 and 6
        let fault_set = BTreeSet::from([3, 4, 5, 6]).try_into().unwrap();
        partition.record_faults(&sector_map, &fault_set, 7)?;

        // now terminate 1, 3 and 5
        let terminations = [sectors[0].clone(), sectors[2].clone(), sectors[4].clone()];
        let termination_block = 3;
        partition.terminate_sectors(termination_block, &terminations)?;

        // pop first termination
        let (result, has_more) = partition.pop_early_terminations(1)?;

        assert!(has_more);
        assert_eq!(result.sectors_processed, 1);
        assert_eq!(result.partitions_processed, 1);
        let terminated_sector = result.sectors.get(&termination_block);
        assert!(terminated_sector.is_some());
        let terminated_sector = terminated_sector.unwrap();
        assert_eq!(terminated_sector, &BTreeSet::from([1]));

        // pop the rest, max_sectors set to 5 but only 2 terminations left, should exit early
        let (result, has_more) = partition.pop_early_terminations(5).unwrap();

        // expect 3 and 5
        let terminated_sector = result.sectors.get(&termination_block);
        assert!(terminated_sector.is_some());
        let terminated_sector = terminated_sector.unwrap();
        assert_eq!(terminated_sector, &BTreeSet::from([3, 5]));
        assert_eq!(result.sectors_processed, 2);
        assert_eq!(result.partitions_processed, 1);

        // expect no more results
        assert!(!has_more);

        // expect early terminations to be empty
        assert!(partition.early_terminations.is_empty());
        Ok(())
    }
}
