#![allow(dead_code, unused)]
use codec::{Decode, Encode};
use frame_support::{pallet_prelude::*, sp_runtime::BoundedBTreeMap, PalletError};
use primitives_proofs::SectorNumber;

use crate::{
    partition::{PartitionNumber, MAX_PARTITIONS},
    sector::MAX_SECTORS,
};

type MapResult<T> = Result<T, MapError>;

/// Maps deadlines to partition maps.
#[derive(Default)]
pub struct DeadlineSectorMap(BoundedBTreeMap<u64, PartitionSectorMap, ConstU32<MAX_PARTITIONS>>);

/// Maps partitions to sectors.
#[derive(Default)]
pub struct PartitionSectorMap(
    BoundedBTreeMap<
        PartitionNumber,
        BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,
        ConstU32<MAX_PARTITIONS>,
    >,
);

impl PartitionSectorMap {
    /// Records the given sector map at the given partition index, merging
    /// it with any existing sectors if necessary.
    pub fn add(
        &mut self,
        partition_idx: PartitionNumber,
        sector_numbers: BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,
    ) -> MapResult<()> {
        if !self.0.contains_key(&partition_idx) {
            self.0.try_insert(partition_idx, sector_numbers).unwrap();
        } else {
            self.0 = self
                .0
                .clone()
                .try_mutate(|partitions| {
                    for number in sector_numbers.iter() {
                        let p = partitions.get_mut(&partition_idx).unwrap();
                        if !p.contains(number) {
                            p.insert(*number);
                        }
                    }
                })
                .ok_or(MapError::Overflow)?;
        }
        Ok(())
    }

    /// Counts the number of partitions & sectors within the map.
    pub fn count(&mut self) -> MapResult<(u64 /* partitions */, u64 /* sectors */)> {
        let sectors = self.0.iter_mut().try_fold(
            0u64,
            |sectors, (_partition_idx, sector_set)| -> MapResult<u64> {
                let sectors = sectors
                    .checked_add(sector_set.len() as u64)
                    .ok_or(MapError::Overflow)?;
                Ok(sectors)
            },
        )?;
        Ok((self.len() as u64, sectors))
    }

    /// Returns an iterator of the partition numbers in the map.
    pub fn partitions(&self) -> impl Iterator<Item = PartitionNumber> + '_ {
        self.0.keys().copied()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Decode, Encode, RuntimeDebug, PalletError)]
pub enum MapError {
    Overflow,
}
