//! This module holds data structures that map sectors to deadlines.
//! The [`PartitionMap`] structure holds a `BTreeMap` that maps partition numbers to sectors.
//! The [`DeadlineMap`] structure contains a `BTreeMap` that map a `PartitionMap` to a deadline.
use codec::{Decode, Encode};
use frame_support::{pallet_prelude::*, sp_runtime::BoundedBTreeMap, PalletError};
use primitives_proofs::{SectorNumber, MAX_TERMINATIONS_PER_CALL};
use scale_info::TypeInfo;

use crate::{partition::PartitionNumber, sector::MAX_SECTORS};

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
    pub fn try_insert_sectors(
        &mut self,
        partition: PartitionNumber,
        sectors: BoundedBTreeSet<SectorNumber, ConstU32<MAX_TERMINATIONS_PER_CALL>>,
    ) -> Result<(), SectorMapError> {
        if let Some(s) = self.0.get_mut(&partition) {
            // NOTE(@jmg-duarte,24/07/2024): to make the operation a no-op we need to merge both
            // sets into a single one and replace the original one if the bounds weren't broken
            for sector in sectors {
                s.try_insert(sector)
                    .map_err(|_| SectorMapError::FailedToInsertSector)?;
            }
        } else {
            // SAFETY: since they're all bounded, if the bounds mismatch,
            // the error should be caught by the compiler
            self.insert_sectors_into_new_partition(partition, sectors)
                .map_err(|_| SectorMapError::FailedToInsertSector)?;
        }
        Ok(())
    }

    /// Insert the given sectors in the given partition number.
    fn insert_sectors_into_new_partition(
        &mut self,
        partition_number: PartitionNumber,
        sectors: BoundedBTreeSet<SectorNumber, ConstU32<MAX_TERMINATIONS_PER_CALL>>,
    ) -> Result<(), SectorMapError> {
        let mut new_sectors = BoundedBTreeSet::new();
        for sector in sectors {
            new_sectors
                .try_insert(sector)
                .map_err(|_| SectorMapError::FailedToInsertSector)?;
        }
        self.0
            .try_insert(partition_number, new_sectors)
            .map_err(|_| SectorMapError::FailedToInsertSector)?;
        Ok(())
    }
}

// Maps deadlines to partitions (which then maps to sectors).
#[derive(RuntimeDebug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone)]
pub struct DeadlineSectorMap(
    BoundedBTreeMap<
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
    /// returns an Err (and is a noop) if the new length of the map exceeds S.
    pub fn try_insert(
        &mut self,
        deadline_index: u64,
        partition: PartitionNumber,
        sectors: BoundedBTreeSet<SectorNumber, ConstU32<MAX_TERMINATIONS_PER_CALL>>,
    ) -> Result<(), SectorMapError> {
        match self.0.get_mut(&deadline_index) {
            Some(p_map) => p_map.try_insert_sectors(partition, sectors),
            None => {
                // create new partition map entry
                let mut p_map = PartitionMap::new();
                p_map
                    .try_insert_sectors(partition, sectors)
                    .map_err(|_| SectorMapError::FailedToInsertSector)?;
                self.0
                    .try_insert(deadline_index, p_map)
                    .map_err(|_| SectorMapError::FailedToInsertSector)?;
                Ok(())
            }
        }
    }
}

#[derive(Decode, Encode, PalletError, TypeInfo, RuntimeDebug)]
pub enum SectorMapError {
    /// Emitted when trying to insert sector(s) fails.
    FailedToInsertSector,
}

#[cfg(test)]
mod test {
    extern crate alloc;

    use alloc::collections::BTreeSet;

    use sp_core::bounded_btree_map;

    use super::*;

    #[test]
    fn partition_map_add_sectors() {
        let mut map = PartitionMap::new();

        let partition = 0;
        let sectors = [1, 2, 3];
        let _ = map.try_insert_sectors(partition, create_set(&sectors));
        expect_sectors_exact(&map, partition, &sectors);

        let sectors = [4, 5, 6];
        let _ = map.try_insert_sectors(partition, create_set(&sectors));
        expect_sectors_partial(&map, partition, &sectors);

        let partition = 1;
        let sectors = [7, 8, 9];
        let _ = map.try_insert_sectors(partition, create_set(&sectors));
        expect_sectors_partial(&map, partition, &sectors);
    }

    #[test]
    fn partition_map_duplicated_sectors() {
        let mut map = PartitionMap::new();
        let partition = 0;
        let sectors = [1, 2, 3];

        let _ = map.try_insert_sectors(partition, create_set(&sectors));
        expect_sectors_exact(&map, partition, &sectors);
        // This call is a no-op since all sectors are already in the partition
        let _ = map.try_insert_sectors(partition, create_set(&sectors));
        expect_sectors_exact(&map, partition, &sectors);

        let partition = 1;
        let sectors = [4, 5, 6];
        let _ = map.try_insert_sectors(partition, create_set(&sectors));
        expect_sectors_exact(&map, partition, &sectors);
    }

    #[test]
    fn partition_map_fail_large_input() {
        let partition = 0;

        // Create a map that is at the limit of the number of sectors
        let original_sectors = (0..MAX_SECTORS as u64).collect::<Vec<_>>();
        let set = create_set::<MAX_SECTORS>(&original_sectors);
        let map = bounded_btree_map!(partition => set);
        let mut map = PartitionMap(map);

        // Try to insert a new partition with a single sector
        let sectors = [u64::MAX]; // We know that this sector is not in the map
        assert!(map
            .try_insert_sectors(partition, create_set(&sectors))
            .is_err());

        // Check that map is still the same. It's faster to check the length
        // instead of going through all elements.
        assert_eq!(map.0.get(&partition).unwrap().len(), original_sectors.len());
    }

    #[test]
    fn deadline_sector_map_add_sectors() {
        let mut map = DeadlineSectorMap::new();

        let deadline = 0;
        let partition = 0;
        let sectors = [1, 2, 3];
        let _ = map.try_insert(deadline, partition, create_set(&sectors));
        expect_deadline_sectors_exact(&map, deadline, partition, &sectors);

        let sectors = [4, 5, 6];
        let _ = map.try_insert(deadline, partition, create_set(&sectors));
        expect_deadline_sectors_partial(&map, deadline, partition, &sectors);

        let partition = 1;
        let sectors = [1, 2, 3];
        let _ = map.try_insert(deadline, partition, create_set(&sectors));
        expect_deadline_sectors_exact(&map, deadline, partition, &sectors);

        let sectors = [4, 5, 6];
        let _ = map.try_insert(deadline, partition, create_set(&sectors));
        expect_deadline_sectors_partial(&map, deadline, partition, &sectors);

        let deadline = 1;
        let partition = 1;
        let sectors = [7, 8, 9];
        let _ = map.try_insert(deadline, partition, create_set(&sectors));
        expect_deadline_sectors_exact(&map, deadline, partition, &sectors);
    }

    #[test]
    fn deadline_sector_map_duplicated() {
        let mut map = DeadlineSectorMap::new();

        let deadline = 0;
        let partition = 0;
        let sectors = [1, 2, 3];
        let _ = map.try_insert(deadline, partition, create_set(&sectors));
        expect_deadline_sectors_exact(&map, deadline, partition, &sectors);

        let sectors = [1, 2, 3];
        let _ = map.try_insert(deadline, partition, create_set(&sectors));
        expect_deadline_sectors_exact(&map, deadline, partition, &sectors);
    }

    /// This is a helper function to easily create a set of sectors.
    fn create_set<const T: u32>(sectors: &[u64]) -> BoundedBTreeSet<SectorNumber, ConstU32<T>> {
        let sectors = sectors.iter().cloned().collect::<BTreeSet<_>>();
        BoundedBTreeSet::try_from(sectors).unwrap()
    }

    /// Check if map contains the expected partition and sectors. This function
    /// panics if any passed sectors are not found in the partition.
    fn expect_sectors_partial(map: &PartitionMap, partition: PartitionNumber, sectors: &[u64]) {
        match map.0.get(&partition) {
            Some(a) => {
                sectors.iter().for_each(|s| {
                    if !a.contains(s) {
                        panic!("sector {} not found in partition {}", s, partition);
                    }
                });
            }
            None => panic!("partition {partition} not found"),
        }
    }

    /// Check if map contains the expected partition and sectors. This function
    /// panics if the actual sectors do not equal the expected sectors.
    fn expect_sectors_exact(map: &PartitionMap, partition: PartitionNumber, sectors: &[u64]) {
        match map.0.get(&partition) {
            Some(actual) => {
                let expected = sectors.iter().copied().collect::<BTreeSet<_>>();
                assert_eq!(&expected, actual.as_ref());
            }
            None => panic!("partition {partition} not found"),
        }
    }

    /// Check if map contains the expected deadline, partition and sectors. The
    /// function panics if there are any sectors missing.
    fn expect_deadline_sectors_partial(
        map: &DeadlineSectorMap,
        deadline: u64,
        partition: PartitionNumber,
        sectors: &[u64],
    ) {
        match map.0.get(&deadline) {
            Some(p_map) => expect_sectors_partial(p_map, partition, sectors),
            None => panic!("deadline {deadline} not found"),
        }
    }

    /// Check if map contains the expected deadline, partition and sectors. The
    /// function panics if sectors are not exact.
    fn expect_deadline_sectors_exact(
        map: &DeadlineSectorMap,
        deadline: u64,
        partition: PartitionNumber,
        sectors: &[u64],
    ) {
        match map.0.get(&deadline) {
            Some(p_map) => expect_sectors_exact(p_map, partition, sectors),
            None => panic!("deadline {deadline} not found"),
        }
    }
}
