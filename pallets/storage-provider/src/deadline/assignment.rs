extern crate alloc;

use alloc::{collections::BinaryHeap, vec, vec::Vec};
use core::cmp::Ordering;

use crate::{
    deadline::{Deadline, DeadlineError},
    sector::SectorOnChainInfo,
};

const LOG_TARGET: &'static str = "runtime::storage_provider::assignment";

/// Intermediary data structure used to assign deadlines to sectors.
struct DeadlineAssignmentInfo {
    /// The deadline index.
    index: usize,
    /// The number of live sectors (i.e. sectors that have *not* been terminated) in the deadline.
    live_sectors: u64,
    /// The total number of sectors in the deadline (may include terminated ones).
    total_sectors: u64,
}

impl DeadlineAssignmentInfo {
    /// Returns the amount of partitions after adding 1 sector to total sectors.
    fn partitions_after_assignment(&self, partition_size: u64) -> u64 {
        let total_sectors = self.total_sectors + 1; // after assignment
        total_sectors.div_ceil(partition_size)
    }

    /// Returns the amount of partitions after adding 1 sector to live sectors.
    fn compact_partitions_after_assignment(&self, partition_size: u64) -> u64 {
        let live_sectors = self.live_sectors + 1; // after assignment
        live_sectors.div_ceil(partition_size)
    }

    /// Partitions size = maximum amount of sectors in a single partition.
    /// total_sectors % partition size is zero if the partition is full.
    /// Example 1: partition size = 10, total sectors = 8; 8 % 10 = 8 -> not full
    /// Example 2: partition size = 10, total sectors = 10; 10 % 10 = 0 -> full
    fn is_full_now(&self, partition_size: u64) -> bool {
        self.total_sectors % partition_size == 0
    }

    /// Determines if the maximum amount of partitions is reached. The max_partitions value is passed into this function.
    fn max_partitions_reached(&self, partition_size: u64, max_partitions: u64) -> bool {
        self.total_sectors >= partition_size * max_partitions
    }
}

/// Reference: https://github.com/filecoin-project/builtin-actors/blob/8d957d2901c0f2044417c268f0511324f591cb92/actors/miner/src/deadline_assignment.rs#L47
fn cmp(a: &DeadlineAssignmentInfo, b: &DeadlineAssignmentInfo, partition_size: u64) -> Ordering {
    // When assigning partitions to deadlines, we're trying to optimize the
    // following:
    //
    // First, avoid increasing the maximum number of partitions in any
    // deadline, across all deadlines, after compaction. This would
    // necessitate buying a new GPU.
    //
    // Second, avoid forcing the miner to repeatedly compact partitions. A
    // miner would be "forced" to compact a partition when a the number of
    // partitions in any given deadline goes above the current maximum
    // number of partitions across all deadlines, and compacting that
    // deadline would then reduce the number of partitions, reducing the
    // maximum.
    //
    // At the moment, the only "forced" compaction happens when either:
    //
    // 1. Assignment of the sector into any deadline would force a
    //    compaction.
    // 2. The chosen deadline has at least one full partition's worth of
    //    terminated sectors and at least one fewer partition (after
    //    compaction) than any other deadline.
    //
    // Third, we attempt to assign "runs" of sectors to the same partition
    // to reduce the size of the bitfields.
    //
    // Finally, we try to balance the number of sectors (thus partitions)
    // assigned to any given deadline over time.

    // Summary:
    //
    // 1. Assign to the deadline that will have the _least_ number of
    //    post-compaction partitions (after sector assignment).
    // 2. Assign to the deadline that will have the _least_ number of
    //    pre-compaction partitions (after sector assignment).
    // 3. Assign to a deadline with a non-full partition.
    //    - If both have non-full partitions, assign to the most full one (stable assortment).
    // 4. Assign to the deadline with the least number of live sectors.
    // 5. Assign sectors to the deadline with the lowest index first.

    // If one deadline would end up with fewer partitions (after
    // compacting), assign to that one. This ensures we keep the maximum
    // number of partitions in any given deadline to a minimum.
    //
    // Technically, this could increase the maximum number of partitions
    // before compaction. However, that can only happen if the deadline in
    // question could save an entire partition by compacting. At that point,
    // the miner should compact the deadline.
    a.compact_partitions_after_assignment(partition_size)
        .cmp(&b.compact_partitions_after_assignment(partition_size))
        .then_with(|| {
            // If, after assignment, neither deadline would have fewer
            // post-compaction partitions, assign to the deadline with the fewest
            // pre-compaction partitions (after assignment). This will put off
            // compaction as long as possible.
            a.partitions_after_assignment(partition_size)
                .cmp(&b.partitions_after_assignment(partition_size))
        })
        .then_with(|| {
            // Ok, we'll end up with the same number of partitions any which way we
            // go. Try to fill up a partition instead of opening a new one.
            a.is_full_now(partition_size)
                .cmp(&b.is_full_now(partition_size))
        })
        .then_with(|| {
            // Either we have two open partitions, or neither deadline has an open
            // partition.

            // If we have two open partitions, fill the deadline with the most-full
            // open partition. This helps us assign runs of sequential sectors into
            // the same partition.
            if !a.is_full_now(partition_size) && !b.is_full_now(partition_size) {
                a.total_sectors.cmp(&b.total_sectors).reverse()
            } else {
                Ordering::Equal
            }
        })
        .then_with(|| {
            // Otherwise, assign to the deadline with the least live sectors. This
            // will break the tie in one of the two immediately preceding
            // conditions.
            a.live_sectors.cmp(&b.live_sectors)
        })
        .then_with(|| {
            // Finally, fall back on the deadline index.
            a.index.cmp(&b.index)
        })
}

/// Assigns partitions to deadlines, first filling partial partitions, then
/// adding new partitions to deadlines with the fewest live sectors.
///
/// ## Returns
///
/// - `Vec<Vec<SectorOnChainInfo<BlockNumber>>>`: A vector of vectors, where:
///   - The outer vector has a length equal to the number of deadlines (`w_post_period_deadlines`).
///   - Each inner vector contains the `SectorOnChainInfo` structures assigned to that deadline.
///   - Deadlines that weren't assigned any sectors will have an empty inner vector.
///
/// The successful return value effectively represents a mapping of deadlines to their assigned sectors.
pub fn assign_deadlines<BlockNumber>(
    max_partitions: u64,
    partition_size: u64,
    deadlines: &[Option<Deadline<BlockNumber>>],
    sectors: &[SectorOnChainInfo<BlockNumber>],
    w_post_period_deadlines: u64,
) -> Result<Vec<Vec<SectorOnChainInfo<BlockNumber>>>, DeadlineError>
where
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    log::debug!(target: LOG_TARGET,"deadlines len: {}, sectors len: {}", deadlines.len(), sectors.len());
    let mut nones = 0;
    for dl in deadlines {
        if dl.is_none() {
            nones += 1;
        }
    }
    log::debug!(target: LOG_TARGET,"deadlines that are none: {nones}");
    struct Entry {
        partition_size: u64,
        info: DeadlineAssignmentInfo,
    }

    impl PartialEq for Entry {
        fn eq(&self, other: &Self) -> bool {
            self.cmp(other) == Ordering::Equal
        }
    }

    impl Eq for Entry {}

    impl PartialOrd for Entry {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }

    impl Ord for Entry {
        fn cmp(&self, other: &Self) -> Ordering {
            // we're using a max heap instead of a min heap, so we need to reverse the ordering
            cmp(&self.info, &other.info, self.partition_size).reverse()
        }
    }

    let mut heap: BinaryHeap<Entry> = deadlines
        .iter()
        .enumerate()
        .filter_map(|(index, deadline)| deadline.as_ref().map(|dl| (index, dl)))
        .map(|(index, deadline)| Entry {
            partition_size,
            info: DeadlineAssignmentInfo {
                index,
                live_sectors: deadline.live_sectors,
                total_sectors: deadline.total_sectors,
            },
        })
        .collect();

    assert!(!heap.is_empty());

    let mut deadlines = vec![Vec::new(); w_post_period_deadlines as usize];

    for sector in sectors {
        let info = &mut heap
            .peek_mut()
            .ok_or(DeadlineError::CouldNotConstructDeadlineInfo)?
            .info;

        if info.max_partitions_reached(partition_size, max_partitions) {
            return Err(DeadlineError::MaxPartitionsReached);
        }

        deadlines[info.index].push(sector.clone());
        info.live_sectors += 1;
        info.total_sectors += 1;
    }

    Ok(deadlines)
}

#[cfg(test)]
mod tests {
    use frame_support::BoundedVec;
    use primitives_proofs::RegisteredSealProof;

    use crate::{
        deadline::{assign_deadlines, Deadline},
        sector::SectorOnChainInfo,
    };

    impl Default for SectorOnChainInfo<u64> {
        fn default() -> Self {
            Self {
                sector_number: 1,
                seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
                sealed_cid: BoundedVec::new(),
                activation: 1,
                expiration: 1,
                unsealed_cid: BoundedVec::new(),
            }
        }
    }

    #[test]
    fn test_deadline_assignment() {
        const PARTITION_SIZE: u64 = 4;
        const MAX_PARTITIONS: u64 = 100;

        #[derive(Clone)]
        struct Spec {
            live_sectors: u64,
            dead_sectors: u64,
            expect_sectors: Vec<u64>,
        }

        struct TestCase {
            sectors: u64,
            deadlines: Vec<Option<Spec>>,
        }
        let test_cases = [
            // Even assignment and striping.
            TestCase {
                sectors: 10,
                deadlines: vec![
                    Some(Spec {
                        dead_sectors: 0,
                        live_sectors: 0,
                        expect_sectors: vec![0, 1, 2, 3, 8, 9],
                    }),
                    Some(Spec {
                        dead_sectors: 0,
                        live_sectors: 0,
                        expect_sectors: vec![4, 5, 6, 7],
                    }),
                ],
            },
            // Fill non-full first
            TestCase {
                sectors: 5,
                deadlines: vec![
                    Some(Spec {
                        dead_sectors: 0,
                        live_sectors: 0,
                        expect_sectors: vec![3, 4],
                    }),
                    None, // expect nothing.
                    None,
                    Some(Spec {
                        dead_sectors: 0,
                        live_sectors: 1,
                        expect_sectors: vec![0, 1, 2],
                    }),
                ],
            },
            // Assign to deadline with least number of live partitions.
            TestCase {
                sectors: 1,
                deadlines: vec![
                    // 2 live partitions. +1 would add another.
                    Some(Spec {
                        dead_sectors: 0,
                        live_sectors: 8,
                        expect_sectors: vec![],
                    }),
                    // 2 live partitions. +1 wouldn't add another.
                    // 1 dead partition.
                    Some(Spec {
                        dead_sectors: 5,
                        live_sectors: 7,
                        expect_sectors: vec![0],
                    }),
                ],
            },
            // Avoid increasing max partitions. Both deadlines have the same
            // number of partitions post-compaction, but deadline 1 has
            // fewer pre-compaction.
            TestCase {
                sectors: 1,
                deadlines: vec![
                    // one live, one dead.
                    Some(Spec {
                        dead_sectors: 4,
                        live_sectors: 4,
                        expect_sectors: vec![],
                    }),
                    // 1 live partitions. +1 would add another.
                    Some(Spec {
                        dead_sectors: 0,
                        live_sectors: 4,
                        expect_sectors: vec![0],
                    }),
                ],
            },
            // With multiple open partitions, assign to most full first.
            TestCase {
                sectors: 1,
                deadlines: vec![
                    Some(Spec {
                        dead_sectors: 0,
                        live_sectors: 1,
                        expect_sectors: vec![],
                    }),
                    Some(Spec {
                        dead_sectors: 0,
                        live_sectors: 2,
                        expect_sectors: vec![0],
                    }),
                ],
            },
            // dead sectors also count
            TestCase {
                sectors: 1,
                deadlines: vec![
                    Some(Spec {
                        dead_sectors: 0,
                        live_sectors: 1,
                        expect_sectors: vec![],
                    }),
                    Some(Spec {
                        dead_sectors: 2,
                        live_sectors: 0,
                        expect_sectors: vec![0],
                    }),
                ],
            },
            // dead sectors really do count.
            TestCase {
                sectors: 1,
                deadlines: vec![
                    Some(Spec {
                        dead_sectors: 1,
                        live_sectors: 0,
                        expect_sectors: vec![],
                    }),
                    Some(Spec {
                        dead_sectors: 2,
                        live_sectors: 0,
                        expect_sectors: vec![0],
                    }),
                ],
            },
            // when partitions are equally full, assign based on live sectors.
            TestCase {
                sectors: 1,
                deadlines: vec![
                    Some(Spec {
                        dead_sectors: 1,
                        live_sectors: 1,
                        expect_sectors: vec![],
                    }),
                    Some(Spec {
                        dead_sectors: 2,
                        live_sectors: 0,
                        expect_sectors: vec![0],
                    }),
                ],
            },
        ];

        for (nth_tc, tc) in test_cases.iter().enumerate() {
            let deadlines: Vec<Option<Deadline<u64>>> = tc
                .deadlines
                .iter()
                .cloned()
                .map(|maybe_dl| {
                    maybe_dl.map(|dl| Deadline {
                        live_sectors: dl.live_sectors,
                        total_sectors: dl.live_sectors + dl.dead_sectors,
                        ..Default::default()
                    })
                })
                .collect();

            let sectors: Vec<SectorOnChainInfo<u64>> = (0..tc.sectors)
                .map(|i| SectorOnChainInfo {
                    sector_number: i,
                    ..Default::default()
                })
                .collect();

            let assignment =
                assign_deadlines(MAX_PARTITIONS, PARTITION_SIZE, &deadlines, &sectors, 48).unwrap();
            for (i, sectors) in assignment.iter().enumerate() {
                if let Some(Some(dl)) = tc.deadlines.get(i) {
                    assert_eq!(
                        dl.expect_sectors.len(),
                        sectors.len(),
                        "for deadline {}, case {}",
                        i,
                        nth_tc
                    );
                    for (i, &expected_sector_no) in dl.expect_sectors.iter().enumerate() {
                        assert_eq!(sectors[i].sector_number, expected_sector_no);
                    }
                } else {
                    assert!(
                        sectors.is_empty(),
                        "expected no sectors to have been assigned to blacked out deadline"
                    );
                }
            }
        }
    }
}
