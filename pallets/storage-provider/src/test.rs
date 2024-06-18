use crate::{
    mock::{events, new_test_ext, RuntimeEvent, RuntimeOrigin, StorageProvider, Test, ALICE, BOB},
    pallet::{Error, Event, StorageProviders},
    types::{RegisteredPoStProof, StorageProviderInfo},
};

use frame_support::{assert_noop, assert_ok};

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
