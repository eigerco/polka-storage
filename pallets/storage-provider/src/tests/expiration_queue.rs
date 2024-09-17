extern crate alloc;
use alloc::collections::{BTreeMap, BTreeSet};

use primitives_proofs::{RegisteredSealProof, SectorNumber};
use sp_core::ConstU32;
use sp_runtime::{BoundedBTreeMap, BoundedBTreeSet};

use super::BlockNumber;
use crate::{
    expiration_queue::{ExpirationQueue, ExpirationSet},
    sector::{SectorOnChainInfo, MAX_SECTORS},
};

fn on_time_sectors() -> [u64; 3] {
    [5, 8, 9]
}

fn early_sectors() -> [u64; 2] {
    [2, 3]
}

fn default_set() -> ExpirationSet {
    let mut set = ExpirationSet::new();
    set.add(&on_time_sectors(), &early_sectors()).unwrap();
    set
}

/// This is a helper function to easily create a set of sectors.
pub fn create_set<const T: u32>(sectors: &[u64]) -> BoundedBTreeSet<SectorNumber, ConstU32<T>> {
    let sectors = sectors.iter().copied().collect::<BTreeSet<_>>();
    BoundedBTreeSet::try_from(sectors).unwrap()
}

/// Create a single sector used in tests
fn test_sector(
    expiration: BlockNumber,
    sector_number: SectorNumber,
) -> SectorOnChainInfo<BlockNumber> {
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
        create_set::<MAX_SECTORS>(&on_time_sectors())
    );
    assert_eq!(
        set.early_sectors,
        create_set::<MAX_SECTORS>(&early_sectors())
    );
}

#[test]
fn add_sectors_to_non_empty_set() {
    let mut set = default_set();
    set.add(&[6, 7, 11], &[1, 4]).unwrap();

    assert_eq!(
        set.on_time_sectors,
        create_set::<MAX_SECTORS>(&[5, 6, 7, 8, 9, 11])
    );
    assert_eq!(set.early_sectors, create_set::<MAX_SECTORS>(&[1, 2, 3, 4]));
}

#[test]
fn remove_sectors_from_set() {
    let mut set = default_set();
    set.remove(&[9], &[2]);

    assert_eq!(set.on_time_sectors, create_set::<MAX_SECTORS>(&[5, 8,]));
    assert_eq!(set.early_sectors, create_set::<MAX_SECTORS>(&[3]));
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
    // - sector 1 was not rescheduled.
    // - sector 2 already expires before the new expiration
    // - sector 3 expiration changed to the new expiration
    // - sector 4 expiration changed to the new expiration
    // - sector 5 expiration changed to the new expiration
    // - sector 6 was not rescheduled.

    assert_eq!(
        queue.map.get(&2).unwrap().on_time_sectors,
        create_set::<MAX_SECTORS>(&[1])
    );
    assert_eq!(
        queue.map.get(&2).unwrap().early_sectors,
        create_set::<MAX_SECTORS>(&[])
    );

    assert_eq!(
        queue.map.get(&3).unwrap().on_time_sectors,
        create_set::<MAX_SECTORS>(&[2])
    );
    assert_eq!(
        queue.map.get(&3).unwrap().early_sectors,
        create_set::<MAX_SECTORS>(&[])
    );

    assert_eq!(
        queue.map.get(&6).unwrap().on_time_sectors,
        create_set::<MAX_SECTORS>(&[])
    );
    assert_eq!(
        queue.map.get(&6).unwrap().early_sectors,
        create_set::<MAX_SECTORS>(&[3, 4, 5])
    );

    assert_eq!(
        queue.map.get(&13).unwrap().on_time_sectors,
        create_set::<MAX_SECTORS>(&[6])
    );
    assert_eq!(
        queue.map.get(&13).unwrap().early_sectors,
        create_set::<MAX_SECTORS>(&[])
    );
}

#[ignore]
#[test]
fn reschedule_recover_restores_sectors() {
    let sectors = sectors();
    let mut queue = ExpirationQueue::<BlockNumber>::new();
    queue.add_active_sectors(&sectors).unwrap();

    // Fault middle sectors to expire at height 6
    let reschedule_sectors = sectors[1..5].iter().collect::<Vec<_>>();
    queue.reschedule_as_faults(6, &reschedule_sectors).unwrap();

    // Mark faulted sectors as recovered
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

    // TODO(no-ref,@cernicc,17/09/2024): Check that the sectors were restored to
    // correct expiration
}

#[ignore]
#[test]
fn removes_sectors() {
    // TODO(no-ref,@cernicc,17/09/2024): Test `remove_sectors` on the ExpirationQueue
    todo!()
}
