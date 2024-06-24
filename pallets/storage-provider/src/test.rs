use frame_support::{assert_noop, assert_ok, sp_runtime::BoundedVec};

use crate::{
    mock::{
        events, new_test_ext, Balances, RuntimeEvent, RuntimeOrigin, StorageProvider, Test, ALICE,
        BOB,
    },
    pallet::{Error, Event, StorageProviders},
    proofs::{RegisteredPoStProof, RegisteredSealProof},
    sector::{ProveCommitSector, SectorPreCommitInfo},
    storage_provider::StorageProviderInfo,
};

#[test]
fn initial_state() {
    new_test_ext().execute_with(|| {
        assert!(!StorageProviders::<Test>::contains_key(ALICE));
        assert!(!StorageProviders::<Test>::contains_key(BOB));
    })
}

/// Tests if storage provider registration is successful.
#[test]
fn register_sp() {
    new_test_ext().execute_with(|| {
        let peer_id = "storage_provider_1".as_bytes().to_vec();
        let peer_id = BoundedVec::try_from(peer_id).unwrap();
        let window_post_type = RegisteredPoStProof::StackedDRGWindow2KiBV1P1;
        let expected_sector_size = window_post_type.sector_size();
        let expected_partition_sectors = window_post_type.window_post_partitions_sector();
        let expected_sp_info = StorageProviderInfo::new(peer_id.clone(), window_post_type);

        // Register BOB as a storage provider.
        assert_ok!(StorageProvider::register_storage_provider(
            RuntimeOrigin::signed(BOB),
            peer_id.clone(),
            window_post_type,
        ));
        assert!(StorageProviders::<Test>::contains_key(BOB));
        // `unwrap()` should be safe because of the above check.
        let sp_bob = StorageProviders::<Test>::get(BOB).unwrap();

        // Check that storage provider information is correct.
        assert_eq!(sp_bob.info.peer_id, peer_id);
        assert_eq!(sp_bob.info.window_post_proof_type, window_post_type);
        assert_eq!(sp_bob.info.sector_size, expected_sector_size);
        assert_eq!(
            sp_bob.info.window_post_partition_sectors,
            expected_partition_sectors
        );

        // Check that pre commit sectors are empty.
        assert!(sp_bob.pre_committed_sectors.is_empty());
        // Check that no pre commit deposit is made
        assert!(sp_bob.pre_commit_deposits.is_none());
        // Check that sectors are empty.
        assert!(sp_bob.sectors.is_empty());

        // Check that the event triggered
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(
                Event::<Test>::StorageProviderRegistered {
                    owner: BOB,
                    info: expected_sp_info
                }
            )]
        )
    })
}

/// Check that double registration fails
#[test]
fn double_register_sp() {
    new_test_ext().execute_with(|| {
        let peer_id = "storage_provider_1".as_bytes().to_vec();
        let peer_id = BoundedVec::try_from(peer_id).unwrap();
        let window_post_type = RegisteredPoStProof::StackedDRGWindow2KiBV1P1;

        // Register BOB as a storage provider.
        assert_ok!(StorageProvider::register_storage_provider(
            RuntimeOrigin::signed(BOB),
            peer_id.clone(),
            window_post_type,
        ));
        assert!(StorageProviders::<Test>::contains_key(BOB));

        // Try to register BOB again. Should fail
        assert_noop!(
            StorageProvider::register_storage_provider(
                RuntimeOrigin::signed(BOB),
                peer_id.clone(),
                window_post_type,
            ),
            Error::<Test>::StorageProviderExists
        );
    });
}

#[test]
fn pre_commit_sector() {
    new_test_ext().execute_with(|| {
        let peer_id = "storage_provider_1".as_bytes().to_vec();
        let peer_id = BoundedVec::try_from(peer_id).unwrap();
        let window_post_type = RegisteredPoStProof::StackedDRGWindow2KiBV1P1;

        // Register ALICE as a storage provider.
        assert_ok!(StorageProvider::register_storage_provider(
            RuntimeOrigin::signed(ALICE),
            peer_id.clone(),
            window_post_type,
        ));
        assert!(StorageProviders::<Test>::contains_key(ALICE));

        // Check that the event triggered
        assert!(matches!(
            events()[..],
            [RuntimeEvent::StorageProvider(
                Event::<Test>::StorageProviderRegistered { .. }
            )]
        ));

        let sector = SectorPreCommitInfo {
            seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
            sector_number: 1,
            sealed_cid: BoundedVec::default(),
            deal_id: 1,
            expiration: 66,
            unsealed_cid: BoundedVec::default(),
        };

        // Check starting balance
        assert_eq!(Balances::free_balance(ALICE), 100);

        // Run pre commit extrinsic
        assert_ok!(StorageProvider::pre_commit_sector(
            RuntimeOrigin::signed(ALICE),
            sector.clone()
        ));

        // Check that the event triggered
        assert_eq!(
            events(),
            [
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Reserved {
                    who: ALICE,
                    amount: 1
                },),
                RuntimeEvent::StorageProvider(Event::<Test>::SectorPreCommitted {
                    owner: ALICE,
                    sector: sector.clone(),
                })
            ]
        );

        // `unwrap()` should be safe because of the above check.
        let sp_alice = StorageProviders::<Test>::get(ALICE).unwrap();

        assert!(sp_alice.sectors.is_empty()); // not yet proven
        assert!(!sp_alice.pre_committed_sectors.is_empty());
        assert!(sp_alice.pre_commit_deposits.is_some());
        assert_eq!(Balances::free_balance(ALICE), 99);
    });
}

#[test] // failure test
fn double_pre_commit_sector() {
    new_test_ext().execute_with(|| {
        let peer_id = "storage_provider_1".as_bytes().to_vec();
        let peer_id = BoundedVec::try_from(peer_id).unwrap();
        let window_post_type = RegisteredPoStProof::StackedDRGWindow2KiBV1P1;

        // Register ALICE as a storage provider.
        assert_ok!(StorageProvider::register_storage_provider(
            RuntimeOrigin::signed(ALICE),
            peer_id.clone(),
            window_post_type,
        ));
        assert!(StorageProviders::<Test>::contains_key(ALICE));

        // Check that the event triggered
        assert!(matches!(
            events()[..],
            [RuntimeEvent::StorageProvider(
                Event::<Test>::StorageProviderRegistered { .. }
            )]
        ));

        let sector = SectorPreCommitInfo {
            seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
            sector_number: 1,
            sealed_cid: BoundedVec::default(),
            deal_id: 1,
            expiration: 66,
            unsealed_cid: BoundedVec::default(),
        };

        // Run pre commit extrinsic
        assert_ok!(StorageProvider::pre_commit_sector(
            RuntimeOrigin::signed(ALICE),
            sector.clone()
        ));

        // Run same extrinsic, this should fail
        assert_noop!(
            StorageProvider::pre_commit_sector(RuntimeOrigin::signed(ALICE), sector.clone()),
            Error::<Test>::MaxPreCommittedSectorExceeded
        );
    });
}

#[test]
fn prove_commit_sector() {
    new_test_ext().execute_with(|| {
        let peer_id = "storage_provider_1".as_bytes().to_vec();
        let peer_id = BoundedVec::try_from(peer_id).unwrap();
        let window_post_type = RegisteredPoStProof::StackedDRGWindow2KiBV1P1;
        let sector_number = 1;

        // Register ALICE as a storage provider.
        assert_ok!(StorageProvider::register_storage_provider(
            RuntimeOrigin::signed(ALICE),
            peer_id.clone(),
            window_post_type,
        ));
        assert!(StorageProviders::<Test>::contains_key(ALICE));

        let sector = SectorPreCommitInfo {
            seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
            sector_number,
            sealed_cid: BoundedVec::default(),
            deal_id: 1,
            expiration: 66,
            unsealed_cid: BoundedVec::default(),
        };

        // Run pre commit extrinsic
        assert_ok!(StorageProvider::pre_commit_sector(
            RuntimeOrigin::signed(ALICE),
            sector.clone()
        ));

        // check that the deposit has been reserved.
        assert_eq!(Balances::free_balance(ALICE), 99);

        // flush the events
        events();

        // Test prove commits
        let sector = ProveCommitSector {
            sector_number,
            proof: BoundedVec::default(),
        };

        assert_ok!(StorageProvider::prove_commit_sector(
            RuntimeOrigin::signed(ALICE),
            sector
        ));

        assert_eq!(
            events(),
            [
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Unreserved {
                    who: ALICE,
                    amount: 1
                }),
                RuntimeEvent::StorageProvider(Event::<Test>::SectorProven {
                    owner: ALICE,
                    sector_number: sector_number
                })
            ]
        );

        // check that the funds have been released
        assert_eq!(Balances::free_balance(ALICE), 100);
    });
}
