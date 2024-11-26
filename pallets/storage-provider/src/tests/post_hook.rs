extern crate alloc;

use alloc::collections::{BTreeMap, BTreeSet};

use frame_support::{assert_ok, pallet_prelude::Get};
use primitives_proofs::{DealId, SectorNumber};
use sp_core::bounded_vec;

use super::new_test_ext;
use crate::{
    pallet::{Config, Event, StorageProviders},
    sector::{ProveCommitSector, MAX_SECTORS},
    tests::{
        account, events, publish_deals, register_storage_provider, run_to_block, sector_set,
        RuntimeEvent, RuntimeOrigin, SectorPreCommitInfoBuilder, StorageProvider,
        SubmitWindowedPoStBuilder, System, Test, CHARLIE,
    },
};

#[test]
fn advances_deadline() {
    new_test_ext().execute_with(|| {
        let challenge_window = <<Test as Config>::WPoStChallengeWindow as Get<u64>>::get();
        let period_deadlines = <<Test as Config>::WPoStPeriodDeadlines as Get<u64>>::get();
        let storage_provider = CHARLIE;
        register_storage_provider(account(storage_provider));

        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();
        assert_eq!(sp.current_deadline, 0);

        for d in 0..(period_deadlines + 1) {
            run_to_block(sp.proving_period_start + challenge_window * d + 1);
            // Refetch SP's data, it was replaced.
            let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();
            assert_eq!(sp.current_deadline, d % period_deadlines);
        }
    });
}

/// Publish 2 deals, by a 1 Storage Provider.
/// Precommit both of them, prove both of them, but don't submit PoSt.
/// It must detect partitions as faulty.
#[test]
fn marks_partitions_as_faulty() {
    new_test_ext().execute_with(|| {
        // given
        let storage_provider = CHARLIE;
        register_storage_provider(account(storage_provider));
        publish_deals(storage_provider);
        let first_deal = 0;
        let second_deal = 1;
        let first_sector_number = 1;
        let second_sector_number = 2;
        precommit_and_prove(storage_provider, first_deal, first_sector_number);
        precommit_and_prove(storage_provider, second_deal, second_sector_number);
        System::reset_events();

        // when
        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();
        let assigned_deadline_end =
            sp.proving_period_start + <<Test as Config>::WPoStChallengeWindow as Get<u64>>::get();
        run_to_block(assigned_deadline_end + 1);

        // then
        // Refetch SP's data, it was replaced.
        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();
        // Sectors land in the first deadline when we start and prove at block 0 in this case.
        let deadline = &sp.deadlines.due[0];
        // Partitions are filled up from the first partition
        let partition = &deadline.partitions[&0];
        let expected_sectors =
            sector_set::<MAX_SECTORS>(&[first_sector_number, second_sector_number]);
        let faulty_sectors = BTreeSet::from([
            SectorNumber::new(first_sector_number).unwrap(),
            SectorNumber::new(second_sector_number).unwrap(),
        ]);
        assert_eq!(partition.faults.len(), 2);
        assert_eq!(expected_sectors, partition.faults);
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(
                Event::<Test>::PartitionsFaulty {
                    owner: account(storage_provider),
                    faulty_partitions: BTreeMap::from([(0u32, faulty_sectors)]),
                }
            ),]
        );
    });
}

/// Publish 2 deals, by a 1 Storage Provider.
/// Precommit both of them, prove both of them, but don't submit PoSt.
/// It DOES NOT detect partitions as faulty and continues without doing any harm.
#[test]
fn does_not_mark_partitions_as_faulty() {
    new_test_ext().execute_with(|| {
        // given
        let storage_provider = CHARLIE;
        register_storage_provider(account(storage_provider));
        publish_deals(storage_provider);
        let first_deal = 0;
        let second_deal = 1;
        let first_sector_number = 1;
        let second_sector_number = 2;
        precommit_and_prove(storage_provider, first_deal, first_sector_number);
        precommit_and_prove(storage_provider, second_deal, second_sector_number);

        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();
        // +1 because `run_to_block` is exclusive.
        run_to_block(sp.proving_period_start + 1);

        assert_ok!(StorageProvider::submit_windowed_post(
            RuntimeOrigin::signed(account(storage_provider)),
            SubmitWindowedPoStBuilder::default().partition(0).build()
        ));
        System::reset_events();

        // when
        let assigned_deadline_end =
            sp.proving_period_start + <<Test as Config>::WPoStChallengeWindow as Get<u64>>::get();
        run_to_block(assigned_deadline_end + 1);

        // then
        // Refetch SP's data, it was replaced.
        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();
        // Sectors land in the first deadline when we start and prove at block 0 in this case.
        let deadline = &sp.deadlines.due[0];
        // Partitions are filled up from the first partition
        let partition = &deadline.partitions[&0];
        let expected_sectors =
            sector_set::<MAX_SECTORS>(&[first_sector_number, second_sector_number]);

        assert_eq!(partition.faults.len(), 0);
        assert_eq!(expected_sectors, partition.sectors);
        assert_eq!(events(), []);
    });
}

fn precommit_and_prove(storage_provider: &'static str, deal_id: DealId, sector_number: u32) {
    let sector_number = SectorNumber::try_from(sector_number).unwrap();

    let sector = SectorPreCommitInfoBuilder::default()
        .sector_number(sector_number)
        .deals(bounded_vec![deal_id])
        .build();

    StorageProvider::pre_commit_sectors(
        RuntimeOrigin::signed(account(storage_provider)),
        bounded_vec![sector.clone()],
    )
    .unwrap();
    StorageProvider::prove_commit_sectors(
        RuntimeOrigin::signed(account(storage_provider)),
        bounded_vec![ProveCommitSector {
            sector_number,
            proof: bounded_vec![0xde],
        }],
    )
    .unwrap();
}
