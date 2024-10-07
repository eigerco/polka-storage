extern crate alloc;

use alloc::collections::BTreeSet;

use frame_support::{assert_err, assert_ok, pallet_prelude::*, sp_runtime::BoundedBTreeSet};
use primitives_proofs::SectorNumber;
use sp_core::bounded_vec;

use crate::{
    error::GeneralPalletError,
    pallet::{Error, StorageProviders},
    sector::{TerminateSectorsParams, TerminationDeclaration, MAX_SECTORS},
    tests::{
        account, declare_faults::setup_sp_with_one_sector, new_test_ext, run_to_block,
        RuntimeOrigin, StorageProvider, Test, ALICE, BOB,
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
                sectors: BTreeSet::from([0]).try_into().unwrap()
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
                sectors: BTreeSet::from([0]).try_into().unwrap()
            }],
        };

        assert_err!(
            StorageProvider::terminate_sectors(RuntimeOrigin::signed(account(ALICE)), params),
            Error::<Test>::CannotTerminateImmutableDeadline
        );
    })
}

/// Successful terminate sectors extrinsic
#[test]
fn terminate_sectors_success() {
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
                sectors: BTreeSet::from([sector]).try_into().unwrap()
            }],
        };

        assert_ok!(StorageProvider::terminate_sectors(
            RuntimeOrigin::signed(account(ALICE)),
            params
        ));

        let mut sp = StorageProviders::<Test>::get(account(storage_provider))
            .expect("Should be able to get providers info");
        // Get first deadline
        let deadline = sp
            .get_deadlines_mut()
            .load_deadline_mut(deadline as usize)
            .unwrap()
            .clone(); // Clone needed to check `pop_early_terminations` from the partition which takes in `&mut self`
                      // Get partition
        let mut partition = deadline.partitions[&partition_num].clone(); // Clone needed to check `pop_early_terminations` from the partition which takes in `&mut self`
        let expected_terminated: BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>> =
            BTreeSet::from([sector]).try_into().unwrap();
        assert_eq!(partition.terminated, expected_terminated);

        let (result, _) = partition.pop_early_terminations(1000).unwrap();
        assert!(result.is_empty());
    })
}
