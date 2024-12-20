extern crate alloc;

use frame_support::{assert_err, assert_ok, pallet_prelude::*};
use primitives::MAX_SECTORS;
use sp_core::bounded_vec;

use crate::{
    error::GeneralPalletError,
    pallet::{Error, Event, StorageProviders},
    sector::{TerminateSectorsParams, TerminationDeclaration},
    tests::{
        account,
        declare_faults::{
            setup_sp_with_many_sectors_multiple_partitions, setup_sp_with_one_sector,
        },
        events, new_test_ext, run_to_block, sector_set, RuntimeEvent, RuntimeOrigin,
        StorageProvider, Test, ALICE, BOB,
    },
};

#[test]
fn terminate_sectors_fails_sp_not_found() {
    new_test_ext().execute_with(|| {
        // Purposely run extrinsic without registration.
        let params = TerminateSectorsParams {
            terminations: bounded_vec![],
        };
        assert_err!(
            StorageProvider::terminate_sectors(RuntimeOrigin::signed(account(ALICE)), params),
            Error::<Test>::StorageProviderNotFound
        );
    });
}

/// Tries to terminate a sector without registering as a storage provider.
#[test]
fn terminate_sectors_fails_non_existent_partition() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;
        setup_sp_with_one_sector(storage_provider, storage_client);
        let params = TerminateSectorsParams {
            terminations: bounded_vec![TerminationDeclaration {
                deadline: 0,
                partition: 2, // Does not exist
                sectors: sector_set(&[0])
            }],
        };
        assert_err!(
            StorageProvider::terminate_sectors(RuntimeOrigin::signed(account(ALICE)), params),
            Error::<Test>::GeneralPalletError(GeneralPalletError::DeadlineErrorPartitionNotFound)
        );
    });
}

/// Tries to terminate a sector that is not mutable.
#[test]
fn terminate_sectors_fails_deadline_not_mutable() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;
        setup_sp_with_one_sector(storage_provider, storage_client);
        // Run to block where the deadline is not mutable
        // Next deadline opens at 62 (default first)
        // Challenge window is 4
        // Deadline is immutable after open - challenge window = 58
        run_to_block(60);
        let params = TerminateSectorsParams {
            terminations: bounded_vec![TerminationDeclaration {
                deadline: 0,
                partition: 0,
                sectors: sector_set(&[0])
            }],
        };

        assert_err!(
            StorageProvider::terminate_sectors(RuntimeOrigin::signed(account(ALICE)), params),
            Error::<Test>::CannotTerminateImmutableDeadline
        );
    })
}

/// Successful terminate sectors extrinsic with a single sector
#[test]
fn terminate_sectors_success_single_sector() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;
        setup_sp_with_one_sector(storage_provider, storage_client);

        let deadline = 0;
        let partition_num = 0;
        let sector = 0;
        let params = TerminateSectorsParams {
            terminations: bounded_vec![TerminationDeclaration {
                deadline,
                partition: partition_num,
                sectors: sector_set(&[sector])
            }],
        };

        assert_ok!(StorageProvider::terminate_sectors(
            RuntimeOrigin::signed(account(ALICE)),
            params
        ));

        let mut sp = StorageProviders::<Test>::get(account(storage_provider))
            .expect("Should be able to get providers info");
        // Get first deadline
        // Clone needed to check `pop_early_terminations` from the partition which takes in `&mut self`
        let deadline = sp
            .get_deadlines_mut()
            .load_deadline_mut(deadline as usize)
            .unwrap()
            .clone();
        // Get partition
        // Clone needed to check `pop_early_terminations` from the partition which takes in `&mut self`
        let mut partition = deadline.partitions[&partition_num].clone();
        let expected_terminated = sector_set::<MAX_SECTORS>(&[sector]);
        assert_eq!(partition.terminated, expected_terminated);

        let (result, has_more) = partition.pop_early_terminations(1000).unwrap();
        assert!(result.is_empty());
        assert_eq!(has_more, false);
    });
}

#[test]
fn terminate_sectors_success_multiple_sectors_partitions_and_deadlines() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;
        setup_sp_with_many_sectors_multiple_partitions(storage_provider, storage_client);

        // Terminate a subset of sectors in the first and second deadline
        // Deadline 0 after setup:
        // live sectors: 5
        //      Partition 0:
        //          sector: 0 <- terminate
        //          sector: 1 <- keep
        //      Partition 1:
        //          sector: 20 <- terminate
        //          sector: 21 <- keep
        //      Partition 2:
        //          sector: 40 <- terminate
        // Deadline 1 after setup:
        // live sectors: 4
        //      Partition 0:
        //          sector: 2 <- keep
        //          sector: 3 <- terminate
        //      Partition 1:
        //          sector: 22 <- keep
        //          sector: 23 <- terminate
        let terminations: BoundedVec<TerminationDeclaration, _> = bounded_vec![
            TerminationDeclaration {
                deadline: 0,
                partition: 0,
                sectors: sector_set(&[0]),
            },
            TerminationDeclaration {
                deadline: 0,
                partition: 1,
                sectors: sector_set(&[20]),
            },
            TerminationDeclaration {
                deadline: 0,
                partition: 2,
                sectors: sector_set(&[40]),
            },
            TerminationDeclaration {
                deadline: 1,
                partition: 0,
                sectors: sector_set(&[3]),
            },
            TerminationDeclaration {
                deadline: 1,
                partition: 1,
                sectors: sector_set(&[23]),
            },
        ];
        let params = TerminateSectorsParams {
            terminations: terminations.clone(), // need clone here to check event later
        };

        assert_ok!(StorageProvider::terminate_sectors(
            RuntimeOrigin::signed(account(ALICE)),
            params
        ));

        // Check that the expected event is emitted
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(Event::SectorsTerminated {
                owner: account(storage_provider),
                terminations
            })]
        );

        let mut sp = StorageProviders::<Test>::get(account(storage_provider))
            .expect("Should be able to get providers info");

        // Check state of first deadline
        let deadline_idx = 0;
        // Check state for first partition
        let deadline = sp
            .get_deadlines_mut()
            .load_deadline_mut(deadline_idx)
            .unwrap();
        let partition = &deadline.partitions[&0];
        let expected_terminated = sector_set::<MAX_SECTORS>(&[0]);
        assert_eq!(partition.terminated, expected_terminated);

        // Check state for second partition
        let deadline = sp
            .get_deadlines_mut()
            .load_deadline_mut(deadline_idx)
            .unwrap();
        let partition = &deadline.partitions[&1];
        let expected_terminated = sector_set::<MAX_SECTORS>(&[20]);
        assert_eq!(partition.terminated, expected_terminated);

        // Check state for last partition
        let deadline = sp
            .get_deadlines_mut()
            .load_deadline_mut(deadline_idx)
            .unwrap();
        let partition = &deadline.partitions[&2];
        let expected_terminated = sector_set::<MAX_SECTORS>(&[40]);
        assert_eq!(partition.terminated, expected_terminated);

        // Check state of second deadline
        let deadline_idx = 1;
        // Check state for first partition
        let deadline = sp
            .get_deadlines_mut()
            .load_deadline_mut(deadline_idx)
            .unwrap();
        let partition = &deadline.partitions[&0];
        let expected_terminated = sector_set::<MAX_SECTORS>(&[3]);
        assert_eq!(partition.terminated, expected_terminated);

        // Check state for second partition
        let deadline = sp
            .get_deadlines_mut()
            .load_deadline_mut(deadline_idx)
            .unwrap();
        let partition = &deadline.partitions[&1];
        let expected_terminated = sector_set::<MAX_SECTORS>(&[23]);
        assert_eq!(partition.terminated, expected_terminated);
    });
}
