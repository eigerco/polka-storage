extern crate alloc;
use alloc::collections::{BTreeMap, BTreeSet};

use primitives_proofs::{RegisteredSealProof, SectorNumber};
use sp_runtime::{BoundedBTreeMap, BoundedBTreeSet};

use super::BlockNumber;
use crate::{
    expiration_queue::{ExpirationQueue, ExpirationSet},
    sector::{SectorOnChainInfo, MAX_SECTORS},
    tests::sector_set,
};

fn on_time_sectors() -> [SectorNumber; 3] {
    [
        SectorNumber::try_from(5).unwrap(),
        SectorNumber::try_from(8).unwrap(),
        SectorNumber::try_from(9).unwrap(),
    ]
}

fn early_sectors() -> [SectorNumber; 2] {
    [
        SectorNumber::try_from(2).unwrap(),
        SectorNumber::try_from(3).unwrap(),
    ]
}

fn default_set() -> ExpirationSet {
    let mut set = ExpirationSet::new();
    set.add(&on_time_sectors(), &early_sectors()).unwrap();
    set
}

/// Create a single sector used in tests
fn test_sector(expiration: BlockNumber, sector_number: u64) -> SectorOnChainInfo<BlockNumber> {
    let sector_number = SectorNumber::try_from(sector_number).unwrap();
    SectorOnChainInfo {
        sector_number,
        seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
        expiration,
        sealed_cid: Default::default(),
        activation: Default::default(),
        unsealed_cid: Default::default(),
    }
}

/// Create a list of sectors used in tests
fn sectors() -> [SectorOnChainInfo<BlockNumber>; 6] {
    [
        test_sector(2, 1),
        test_sector(3, 2),
        test_sector(7, 3),
        test_sector(8, 4),
        test_sector(11, 5),
        test_sector(13, 6),
    ]
}

#[test]
fn add_sectors_to_empty_set() {
    let set = default_set();

    assert_eq!(
        set.on_time_sectors,
        sector_set::<MAX_SECTORS, _>(on_time_sectors().into_iter())
    );
    assert_eq!(
        set.early_sectors,
        sector_set::<MAX_SECTORS, _>(early_sectors().into_iter())
    );
}

#[test]
fn add_sectors_to_non_empty_set() {
    let mut set = default_set();
    set.add(
        &[
            SectorNumber::try_from(6).unwrap(),
            SectorNumber::try_from(7).unwrap(),
            SectorNumber::try_from(11).unwrap(),
        ],
        &[
            SectorNumber::try_from(1).unwrap(),
            SectorNumber::try_from(4).unwrap(),
        ],
    )
    .unwrap();

    assert_eq!(
        set.on_time_sectors,
        sector_set::<MAX_SECTORS, _>([5, 6, 7, 8, 9, 11].into_iter())
    );
    assert_eq!(
        set.early_sectors,
        sector_set::<MAX_SECTORS, _>([1, 2, 3, 4].into_iter())
    );
}

#[test]
fn remove_sectors_from_set() {
    let mut set = default_set();
    set.remove(
        &[SectorNumber::try_from(9).unwrap()],
        &[SectorNumber::try_from(2).unwrap()],
    );

    assert_eq!(
        set.on_time_sectors,
        sector_set::<MAX_SECTORS, _>([5, 8].into_iter())
    );
    assert_eq!(
        set.early_sectors,
        sector_set::<MAX_SECTORS, _>([3].into_iter())
    );
}

#[test]
fn set_is_empty_when_all_sectors_removed() {
    let mut set = ExpirationSet::new();
    assert!(set.is_empty());
    assert_eq!(set.len(), 0);

    set.add(&on_time_sectors(), &early_sectors()).unwrap();
    assert!(!set.is_empty());
    assert_eq!(set.len(), 5);

    set.remove(&on_time_sectors(), &early_sectors());
    assert!(set.is_empty());
    assert_eq!(set.len(), 0);
}

#[test]
fn add_sectors_to_expiration_queue() {
    let mut queue = ExpirationQueue::<BlockNumber>::new();

    queue.add_active_sectors(&sectors()).unwrap();
    assert_eq!(queue.map.len(), 6);
}

#[test]
fn reschedules_sectors_as_faults() {
    let sectors = sectors();
    let mut queue = ExpirationQueue::<BlockNumber>::new();
    queue.add_active_sectors(&sectors).unwrap();

    // Fault middle sectors to expire at height 6
    let reschedule_sectors = sectors[1..5].iter().collect::<Vec<_>>();
    queue.reschedule_as_faults(6, &reschedule_sectors).unwrap();

    // Check that the sectors are in the right place:
    let checks = [
        // - sector 1 was not rescheduled.
        (2, vec![1], vec![]),
        // - sector 2 already expires before the new expiration
        (3, vec![2], vec![]),
        // - sector 3 expiration changed to the new expiration
        // - sector 4 expiration changed to the new expiration
        // - sector 5 expiration changed to the new expiration
        (6, vec![], vec![3, 4, 5]),
        // - sector 6 was not rescheduled.
        (13, vec![6], vec![]),
    ];

    for (expiration_height, on_time, early) in checks {
        let set = queue.map.get(&expiration_height).unwrap();
        assert_eq!(
            set.on_time_sectors,
            sector_set::<MAX_SECTORS, _>(on_time.into_iter())
        );
        assert_eq!(
            set.early_sectors,
            sector_set::<MAX_SECTORS, _>(early.into_iter())
        );
    }
}

#[test]
fn reschedule_recover_restores_sectors() {
    let sectors = sectors();
    let mut queue = ExpirationQueue::<BlockNumber>::new();
    queue.add_active_sectors(&sectors).unwrap();

    // Queue before the faults and recoveries
    let queue_before = queue.clone();

    // Fault middle sectors to expire at height 6
    let reschedule_sectors = sectors[1..5].iter().collect::<Vec<_>>();
    queue.reschedule_as_faults(6, &reschedule_sectors).unwrap();

    // Mark faulty sectors as recovered
    let reschedule_sectors = reschedule_sectors
        .iter()
        .map(|s| s.sector_number)
        .collect::<BTreeSet<_>>();
    let all_sectors = sectors
        .into_iter()
        .map(|s| (s.sector_number, s))
        .collect::<BTreeMap<_, _>>();
    queue
        .reschedule_recovered(
            &BoundedBTreeMap::try_from(all_sectors).unwrap(),
            &BoundedBTreeSet::try_from(reschedule_sectors).unwrap(),
        )
        .unwrap();

    assert_eq!(queue_before, queue);
}

#[ignore]
#[test]
fn removes_sectors() {
    // TODO(109,@cernicc,17/09/2024): Test `remove_sectors` on the ExpirationQueue
    todo!()
}
