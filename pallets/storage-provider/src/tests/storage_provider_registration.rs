use frame_support::{assert_noop, assert_ok};
use primitives::proofs::RegisteredPoStProof;
use sp_runtime::{BoundedVec, DispatchError};

use super::new_test_ext;
use crate::{
    pallet::{Error, Event, StorageProviders},
    storage_provider::StorageProviderInfo,
    tests::{account, events, RuntimeEvent, RuntimeOrigin, StorageProvider, Test, BOB},
};

/// Tests if storage provider registration is successful.
#[test]
fn successful_registration() {
    new_test_ext().execute_with(|| {
        let peer_id = "storage_provider_1".as_bytes().to_vec();
        let peer_id = BoundedVec::try_from(peer_id).unwrap();
        let window_post_type = RegisteredPoStProof::StackedDRGWindow2KiBV1P1;
        let expected_sector_size = window_post_type.sector_size();
        let expected_partition_sectors = window_post_type.window_post_partitions_sector();
        let expected_sp_info = StorageProviderInfo::new(peer_id.clone(), window_post_type);

        // Register BOB as a storage provider.
        assert_ok!(StorageProvider::register_storage_provider(
            RuntimeOrigin::signed(account(BOB)),
            peer_id.clone(),
            window_post_type,
        ));
        assert!(StorageProviders::<Test>::contains_key(account(BOB)));

        // `unwrap()` should be safe because of the above check.
        let sp_bob = StorageProviders::<Test>::get(account(BOB)).unwrap();
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
        assert_eq!(sp_bob.pre_commit_deposits, 0);
        // Check that sectors are empty.
        assert!(sp_bob.sectors.is_empty());
        // Check that the event triggered
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(
                Event::<Test>::StorageProviderRegistered {
                    owner: account(BOB),
                    info: expected_sp_info,
                    // It's calculated according to `calculate_first_proving_period` and is random (because offset)
                    // So first make the test fail, then put a correct value here.
                    proving_period_start: 69,
                },
            )]
        );
    })
}

#[test]
fn fails_should_be_signed() {
    new_test_ext().execute_with(|| {
        let peer_id = "storage_provider_1".as_bytes().to_vec();
        let peer_id = BoundedVec::try_from(peer_id).unwrap();
        let window_post_type = RegisteredPoStProof::StackedDRGWindow2KiBV1P1;

        assert_noop!(
            StorageProvider::register_storage_provider(
                RuntimeOrigin::none(),
                peer_id.clone(),
                window_post_type,
            ),
            DispatchError::BadOrigin
        );
    });
}

#[test]
fn fails_double_register() {
    new_test_ext().execute_with(|| {
        let peer_id = "storage_provider_1".as_bytes().to_vec();
        let peer_id = BoundedVec::try_from(peer_id).unwrap();
        let window_post_type = RegisteredPoStProof::StackedDRGWindow2KiBV1P1;

        // Register BOB as a storage provider.
        assert_ok!(StorageProvider::register_storage_provider(
            RuntimeOrigin::signed(account(BOB)),
            peer_id.clone(),
            window_post_type,
        ));
        assert!(StorageProviders::<Test>::contains_key(account(BOB)));
        // Try to register BOB again. Should fail
        assert_noop!(
            StorageProvider::register_storage_provider(
                RuntimeOrigin::signed(account(BOB)),
                peer_id.clone(),
                window_post_type,
            ),
            Error::<Test>::StorageProviderExists
        );
    });
}
