//! This module holds data structures that map sectors to deadlines.
//! The [`PartitionMap`] structure holds a `BTreeMap` that maps partition numbers to sectors.
//! The [`DeadlineMap`] structure contains a `BTreeMap` that map a `PartitionMap` to a deadline.
use codec::{Decode, Encode};
use frame_support::{pallet_prelude::*, sp_runtime::BoundedBTreeMap};
use primitives::{sector::SectorNumber, PartitionNumber, MAX_SECTORS, MAX_TERMINATIONS_PER_CALL};
use scale_info::TypeInfo;

use crate::error::GeneralPalletError;

const LOG_TARGET: &'static str = "runtime::storage_provider::sector_map";

/// Maximum terminations allowed per extrinsic call, wrapped in [`ConstU32`] to be used as a bound.
///
/// This bound is most useful in:
/// * `Pallet<T>::terminate_sector` — where it provides an upper bound to the
///   number of terminations per extrinsic call.
/// * `DeadlineSectorMap` — where it provides an upper bound to the number of deadlines.
/// * `PartitionMap` — where it provides an upper bound to the number of partitions.
///
/// This bound carries a caveat, remember that `terminate_sector` takes in a list of terminations
/// which carry pairs of deadline index and partition, this means that we may have a list of
/// terminations that all pertain to the same deadline index, pertain to the same partition,
/// or both!
///
/// So, if we are to keep the pair of keys separate (i.e. `deadline_index -> partition -> [sector]`,
/// instead of `(deadline_index, partition) -> [sector]`), we need to take into consideration the
/// edge cases previously described. Making the effective upper bound of `DeadlineSectorMap` equal
/// to `MAX_TERMINATIONS_PER_CALL * MAX_TERMINATIONS_PER_CALL * MAX_SECTORS`.
pub(crate) type MaxTerminationsPerCallBound = ConstU32<MAX_TERMINATIONS_PER_CALL>;

/// Maps partitions to sectors.
///
/// For information about the bounds, check [`MaxTerminationsPerCallBound`].
#[derive(RuntimeDebug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone, Default)]
pub struct PartitionMap(
    pub  BoundedBTreeMap<
        PartitionNumber,
        BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,
        // This structure is only used on `terminate_sectors`, which receives
        // at most MAX_TERMINATIONS_PER_CALL, since each termination only has
        // a single partition, this bound should hold
        MaxTerminationsPerCallBound,
    >,
);

impl PartitionMap {
    /// Construct a new [`PartitionMap`].
    pub fn new() -> Self {
        Default::default()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Inserts a sector in the given partition. Returns whether the value was newly inserted.
    ///
    /// * If the partition did not exist, a new set of sectors will be created.
    /// * If the bounds are broken (partitions or sectors), the operation _IS NOT_ a no-op
    ///   and returns an error.
    /// * If no sectors are passed to be inserted, the operation returns an error and no changes are made.
    pub fn try_insert_sectors(
        &mut self,
        partition: PartitionNumber,
        sectors: BoundedBTreeSet<SectorNumber, ConstU32<MAX_TERMINATIONS_PER_CALL>>,
    ) -> Result<(), GeneralPalletError> {
        if let Some(s) = self.0.get_mut(&partition) {
            // NOTE(@jmg-duarte,24/07/2024): to make the operation a no-op we need to merge both
            // sets into a single one and replace the original one if the bounds weren't broken
            for sector in sectors {
                s.try_insert(sector).map_err(|_| {
                    log::error!(target: LOG_TARGET, "try_insert_sectors: Could not insert {sector:?} into sectors");
                    GeneralPalletError::SectorMapErrorFailedToInsertSector
                })?;
            }
        } else {
            // SAFETY: since they're all bounded, if the bounds mismatch,
            // the error should be caught by the compiler
            self.insert_sectors_into_new_partition(partition, sectors)
                .map_err(|e| {
                    log::error!(target: LOG_TARGET, e:?; "try_insert_sectors: Could not insert sectors into new partition {partition:?}");
                    GeneralPalletError::SectorMapErrorFailedToInsertSector
                })?;
        }
        Ok(())
    }

    /// Insert the given sectors in the given partition number.
    fn insert_sectors_into_new_partition(
        &mut self,
        partition_number: PartitionNumber,
        sectors: BoundedBTreeSet<SectorNumber, ConstU32<MAX_TERMINATIONS_PER_CALL>>,
    ) -> Result<(), GeneralPalletError> {
        let mut new_sectors = BoundedBTreeSet::new();
        for sector in sectors {
            new_sectors.try_insert(sector).map_err(|_| {
                log::error!(target: LOG_TARGET, "insert_sectors_into_new_partition: Could not insert sector {sector:?} into new sectors");
                GeneralPalletError::SectorMapErrorFailedToInsertSector
            })?;
        }
        self.0
            .try_insert(partition_number, new_sectors)
            .map_err(|_| {
                log::error!(target: LOG_TARGET, "insert_sectors_into_new_partition: Could not insert new sectors into partition {partition_number:?}");
                GeneralPalletError::SectorMapErrorFailedToInsertSector
            })?;
        Ok(())
    }
}

// Maps deadlines to partitions (which then maps to sectors).
#[derive(RuntimeDebug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone)]
pub struct DeadlineSectorMap(
    pub  BoundedBTreeMap<
        u64, // Deadline Index
        PartitionMap,
        // Similar to the bound in `PartitionMap`, we have the same `MAX_TERMINATIONS_PER_CALL`
        // while the max bounded size is MAX_TERMINATIONS_PER_CALL^2, we need to consider
        // that a full termination might all pertain to the same deadline, like
        // `[{deadline: 1, ...}, {deadline: 1, ...}, {deadline: 1, ...}]`
        // or to the same partition
        // `[{..., partition: 1, ...}, {..., partition: 1, ...}, {..., partition: 1, ...}]`
        // or mixed! Hence, the same bound (MAX_TERMINATIONS_PER_CALL) needs to be applied to both
        MaxTerminationsPerCallBound,
    >,
);

// NOTE(@jmg-duarte,24/07/2024): big incantantion just to forward an iterator implementation
impl<'a> IntoIterator for &'a mut DeadlineSectorMap {
    type Item = (&'a u64, &'a mut PartitionMap);

    type IntoIter =
        <&'a mut BoundedBTreeMap<u64, PartitionMap, MaxTerminationsPerCallBound> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

impl DeadlineSectorMap {
    /// Construct a new [`DeadlineSectorMap`].
    pub fn new() -> Self {
        Self(BoundedBTreeMap::new())
    }

    /// Attempts to insert new sectors into a partition.
    /// If the partition does not exist this partition will be created and the sectors added.
    ///
    /// Returns an Err (and is a no-op) if the new length of the map exceeds S.
    pub fn try_insert(
        &mut self,
        deadline_index: u64,
        partition: PartitionNumber,
        sectors: BoundedBTreeSet<SectorNumber, ConstU32<MAX_TERMINATIONS_PER_CALL>>,
    ) -> Result<(), GeneralPalletError> {
        match self.0.get_mut(&deadline_index) {
            Some(p_map) => p_map.try_insert_sectors(partition, sectors),
            None => {
                // create new partition map entry
                let mut p_map = PartitionMap::new();
                p_map.try_insert_sectors(partition, sectors)?;

                self.0
                    .try_insert(deadline_index, p_map)
                    .map_err(|_| {
                        log::error!(target: LOG_TARGET, "try_insert: Could not insert partition map into deadline index {deadline_index}");
                        GeneralPalletError::SectorMapErrorFailedToInsertPartition
                    })?;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod test {
    extern crate alloc;

    use alloc::collections::BTreeSet;

    use super::*;
    use crate::tests::sector_set;

    #[test]
    fn partition_map_add_sectors() {
        let mut map = PartitionMap::new();

        let partition = 0;
        let sectors = [1, 2, 3];
        let _ = map.try_insert_sectors(partition, sector_set(&sectors));
        expect_sectors_exact(&map, partition, &sectors);

        let sectors = [4, 5, 6];
        let _ = map.try_insert_sectors(partition, sector_set(&sectors));
        expect_sectors_partial(&map, partition, &sectors);

        let partition = 1;
        let sectors = [7, 8, 9];
        let _ = map.try_insert_sectors(partition, sector_set(&sectors));
        expect_sectors_partial(&map, partition, &sectors);
    }

    #[test]
    fn partition_map_duplicated_sectors() {
        let mut map = PartitionMap::new();
        let partition = 0;
        let sectors = [1, 2, 3];

        let _ = map.try_insert_sectors(partition, sector_set(&sectors));
        expect_sectors_exact(&map, partition, &sectors);
        // This call is a no-op since all sectors are already in the partition
        let _ = map.try_insert_sectors(partition, sector_set(&sectors));
        expect_sectors_exact(&map, partition, &sectors);

        let partition = 1;
        let sectors = [4, 5, 6];
        let _ = map.try_insert_sectors(partition, sector_set(&sectors));
        expect_sectors_exact(&map, partition, &sectors);
    }

    #[test]
    fn deadline_sector_map_add_sectors() {
        let mut map = DeadlineSectorMap::new();

        let deadline = 0;
        let partition = 0;
        let sectors = [1, 2, 3];
        let _ = map.try_insert(deadline, partition, sector_set(&sectors));
        expect_deadline_sectors_exact(&map, deadline, partition, &sectors);

        let sectors = [4, 5, 6];
        let _ = map.try_insert(deadline, partition, sector_set(&sectors));
        expect_deadline_sectors_partial(&map, deadline, partition, &sectors);

        let partition = 1;
        let sectors = [1, 2, 3];
        let _ = map.try_insert(deadline, partition, sector_set(&sectors));
        expect_deadline_sectors_exact(&map, deadline, partition, &sectors);

        let sectors = [4, 5, 6];
        let _ = map.try_insert(deadline, partition, sector_set(&sectors));
        expect_deadline_sectors_partial(&map, deadline, partition, &sectors);

        let deadline = 1;
        let partition = 1;
        let sectors = [7, 8, 9];
        let _ = map.try_insert(deadline, partition, sector_set(&sectors));
        expect_deadline_sectors_exact(&map, deadline, partition, &sectors);
    }

    #[test]
    fn deadline_sector_map_duplicated() {
        let mut map = DeadlineSectorMap::new();

        let deadline = 0;
        let partition = 0;
        let sectors = [1, 2, 3];
        let _ = map.try_insert(deadline, partition, sector_set(&sectors));
        expect_deadline_sectors_exact(&map, deadline, partition, &sectors);

        let sectors = [1, 2, 3];
        let _ = map.try_insert(deadline, partition, sector_set(&sectors));
        expect_deadline_sectors_exact(&map, deadline, partition, &sectors);
    }

    /// Checks that items in `expected_sectors` are in the actual partition. Any
    /// extra items that are not in the `expected_sectors` are ignored.
    fn expect_sectors_partial(
        map: &PartitionMap,
        partition: PartitionNumber,
        expected_sectors: &[u32],
    ) {
        match map.0.get(&partition) {
            Some(a) => {
                expected_sectors
                    .into_iter()
                    .copied()
                    .map(|s| s.try_into().unwrap())
                    .enumerate()
                    .for_each(|(idx, s)| {
                        if !a.contains(&s) {
                            panic!(
                                "sector {} (idx: {}) not found in partition {}",
                                s, idx, partition
                            );
                        }
                    });
            }
            None => panic!("partition {partition} not found"),
        }
    }

    /// Checks that all items in `expected_sectors` are in the actual partition.
    /// The actual partition should have no extra or missing items.
    fn expect_sectors_exact(
        map: &PartitionMap,
        partition: PartitionNumber,
        expected_sectors: &[u32],
    ) {
        match map.0.get(&partition) {
            Some(actual) => {
                let expected = expected_sectors
                    .into_iter()
                    .copied()
                    .map(|s| s.try_into().unwrap())
                    .collect::<BTreeSet<_>>();
                assert_eq!(expected.len(), actual.len());
                assert_eq!(&expected, actual.as_ref());
            }
            None => panic!("partition {partition} not found"),
        }
    }

    /// Checks that items in `expected_sectors` are in the actual partition
    /// deadline. Any extra items that are not in the `expected_sectors` are
    /// ignored.
    fn expect_deadline_sectors_partial(
        map: &DeadlineSectorMap,
        deadline: u64,
        partition: PartitionNumber,
        expected_sectors: &[u32],
    ) {
        match map.0.get(&deadline) {
            Some(p_map) => expect_sectors_partial(p_map, partition, expected_sectors),
            None => panic!("deadline {deadline} not found"),
        }
    }

    /// Checks that all items in `expected_sectors` are in the actual partition
    /// deadline. The actual partition should have no extra or missing items.
    fn expect_deadline_sectors_exact(
        map: &DeadlineSectorMap,
        deadline: u64,
        partition: PartitionNumber,
        expected_sectors: &[u32],
    ) {
        match map.0.get(&deadline) {
            Some(p_map) => expect_sectors_exact(p_map, partition, expected_sectors),
            None => panic!("deadline {deadline} not found"),
        }
    }
}
