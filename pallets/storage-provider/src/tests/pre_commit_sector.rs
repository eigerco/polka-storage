use frame_support::{assert_noop, assert_ok};
use sp_runtime::{BoundedVec, DispatchError};

use super::new_test_ext;
use crate::{
    pallet::{Error, Event, StorageProviders},
    sector::SECTORS_MAX,
    tests::{
        account, events, register_storage_provider, run_to_block, Balances, MaxProveCommitDuration,
        MaxSectorExpirationExtension, RuntimeEvent, RuntimeOrigin, SectorPreCommitInfoBuilder,
        StorageProvider, Test, ALICE,
    },
};

#[test]
fn successfully_precommited() {
    new_test_ext().execute_with(|| {
        // Register ALICE as a storage provider.
        let storage_provider = ALICE;
        register_storage_provider(account(storage_provider));

        // Sector to be pre-committed.
        let sector = SectorPreCommitInfoBuilder::default()
            .sector_number(1)
            .deals([0, 1])
            .sealed_cid("sealed_cid")
            .unsealed_cid("unsealed_cid")
            .build();

        // Check starting balance
        assert_eq!(Balances::free_balance(account(storage_provider)), 100);

        // Run pre commit extrinsic
        StorageProvider::pre_commit_sector(
            RuntimeOrigin::signed(account(storage_provider)),
            sector.clone(),
        )
        .expect("Pre commit failed");

        // Check that the events were triggered
        assert_eq!(
            events(),
            [
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Reserved {
                    who: account(storage_provider),
                    amount: 1
                },),
                RuntimeEvent::StorageProvider(Event::<Test>::SectorPreCommitted {
                    owner: account(storage_provider),
                    sector: sector.clone(),
                })
            ]
        );

        let sp_alice = StorageProviders::<Test>::get(account(storage_provider))
            .expect("SP Alice should be present because of the pre-check");

        assert!(sp_alice.sectors.is_empty()); // not yet proven
        assert!(!sp_alice.pre_committed_sectors.is_empty());
        assert_eq!(sp_alice.pre_commit_deposits, 1);
        assert_eq!(Balances::free_balance(account(storage_provider)), 99);
    });
}

#[test]
fn fails_should_be_signed() {
    new_test_ext().execute_with(|| {
        // Sector to be pre-committed
        let sector = SectorPreCommitInfoBuilder::default()
            .sector_number(1)
            .deals([0, 1])
            .sealed_cid("sealed_cid")
            .unsealed_cid("unsealed_cid")
            .build();

        // Run pre commit extrinsic
        assert_noop!(
            StorageProvider::pre_commit_sector(RuntimeOrigin::none(), sector.clone()),
            DispatchError::BadOrigin,
        );
    });
}

#[test]
fn fails_storage_provider_not_found() {
    new_test_ext().execute_with(|| {
        // Sector to be pre-committed
        let sector = SectorPreCommitInfoBuilder::default()
            .sector_number(1)
            .deals([0, 1])
            .sealed_cid("sealed_cid")
            .unsealed_cid("unsealed_cid")
            .build();

        // Run pre commit extrinsic
        assert_noop!(
            StorageProvider::pre_commit_sector(
                RuntimeOrigin::signed(account(ALICE)),
                sector.clone()
            ),
            Error::<Test>::StorageProviderNotFound,
        );
    });
}

#[test]
fn fails_sector_number_already_used() {
    new_test_ext().execute_with(|| {
        // Register ALICE as a storage provider.
        let storage_provider = ALICE;
        register_storage_provider(account(storage_provider));

        // Sector to be pre-committed
        let sector = SectorPreCommitInfoBuilder::default()
            .sector_number(1)
            .deals([0, 1])
            .sealed_cid("sealed_cid")
            .unsealed_cid("unsealed_cid")
            .build();

        // Run pre commit extrinsic
        assert_ok!(StorageProvider::pre_commit_sector(
            RuntimeOrigin::signed(account(storage_provider)),
            sector.clone()
        ));
        // Run same extrinsic, this should fail
        assert_noop!(
            StorageProvider::pre_commit_sector(
                RuntimeOrigin::signed(account(storage_provider)),
                sector.clone()
            ),
            Error::<Test>::SectorNumberAlreadyUsed,
        );
    });
}

#[test]
fn fails_invalid_sector() {
    new_test_ext().execute_with(|| {
        // Register ALICE as a storage provider.
        let storage_provider = ALICE;
        register_storage_provider(account(storage_provider));

        // Sector to be pre-committed
        let sector = SectorPreCommitInfoBuilder::default()
            .sector_number(SECTORS_MAX as u64 + 1)
            .deals([0, 1])
            .sealed_cid("sealed_cid")
            .unsealed_cid("unsealed_cid")
            .build();

        // Run pre commit extrinsic
        assert_noop!(
            StorageProvider::pre_commit_sector(
                RuntimeOrigin::signed(account(storage_provider)),
                sector.clone()
            ),
            Error::<Test>::InvalidSector,
        );
    });
}

#[test]
fn fails_invalid_cid() {
    new_test_ext().execute_with(|| {
        // Register ALICE as a storage provider.
        let storage_provider = ALICE;
        register_storage_provider(account(storage_provider));

        // Sector to be pre-committed
        let mut sector = SectorPreCommitInfoBuilder::default()
            .sector_number(1)
            .deals([0, 1])
            .sealed_cid("sealed_cid")
            .unsealed_cid("unsealed_cid")
            .build();

        // Setting the wrong unseal cid on the sector
        sector.unsealed_cid = BoundedVec::new();

        // Run pre commit extrinsic
        assert_noop!(
            StorageProvider::pre_commit_sector(
                RuntimeOrigin::signed(account(storage_provider)),
                sector.clone()
            ),
            Error::<Test>::InvalidCid,
        );
    });
}

#[test]
fn fails_expiration_before_activation() {
    new_test_ext().execute_with(|| {
        run_to_block(1000);

        // Register ALICE as a storage provider.
        let storage_provider = ALICE;
        register_storage_provider(account(&storage_provider));

        // Sector to be pre-committed
        let sector = SectorPreCommitInfoBuilder::default()
            .sector_number(1)
            .deals([0, 1])
            .sealed_cid("sealed_cid")
            .unsealed_cid("unsealed_cid")
            .expiration(1000)
            .build();

        // Run pre commit extrinsic
        assert_noop!(
            StorageProvider::pre_commit_sector(
                RuntimeOrigin::signed(account(storage_provider)),
                sector.clone()
            ),
            Error::<Test>::ExpirationBeforeActivation,
        );
    });
}

#[test]
fn fails_expiration_too_soon() {
    let current_height = 1000;

    new_test_ext().execute_with(|| {
        run_to_block(current_height);

        // Register ALICE as a storage provider.
        let storage_provider = ALICE;
        register_storage_provider(account(storage_provider));

        // Sector to be pre-committed
        let sector = SectorPreCommitInfoBuilder::default()
            .sector_number(1)
            .deals([0, 1])
            .sealed_cid("sealed_cid")
            .unsealed_cid("unsealed_cid")
            // Set expiration to be in the next block after the maximum
            // allowed activation.
            .expiration(current_height + MaxProveCommitDuration::get() + 1)
            .build();

        assert_noop!(
            StorageProvider::pre_commit_sector(
                RuntimeOrigin::signed(account(storage_provider)),
                sector.clone()
            ),
            Error::<Test>::ExpirationTooSoon,
        );
    });
}

#[test]
fn fails_expiration_too_long() {
    let current_height = 1000;

    new_test_ext().execute_with(|| {
        run_to_block(current_height);

        // Register ALICE as a storage provider.
        let storage_provider = ALICE;
        register_storage_provider(account(storage_provider));

        // Sector to be pre-committed
        let sector = SectorPreCommitInfoBuilder::default()
            .sector_number(1)
            .deals([0, 1])
            .sealed_cid("sealed_cid")
            .unsealed_cid("unsealed_cid")
            // Set expiration to be in the next block after the maximum
            // allowed
            .expiration(current_height + MaxSectorExpirationExtension::get() + 1)
            .build();

        // Run pre commit extrinsic
        assert_noop!(
            StorageProvider::pre_commit_sector(
                RuntimeOrigin::signed(account(storage_provider)),
                sector.clone()
            ),
            Error::<Test>::ExpirationTooLong,
        );
    });
}

// TODO(no-ref,@cernicc,11/07/2024): Based on the current setup I can't get
// this test to pass. That is because the `SectorMaximumLifetime` is longer
// then the bound for `ExpirationTooLong`. Is the test wrong? is the
// implementation wrong?
//
// #[test]
// fn fails_max_sector_lifetime_exceeded() {
//     let current_height = 1000;

//     new_test_ext_with_block(current_height).execute_with(|| {
//         // Register ALICE as a storage provider.
//         let storage_provider = ALICE;
//         register_storage_provider(account(storage_provider));

//         // Sector to be pre-committed
//         let sector = SectorPreCommitInfoBuilder::default()
//             .sector_number(1)
//             .deals([0, 1])
//             .sealed_cid("sealed_cid")
//             .unsealed_cid("unsealed_cid")
//             .expiration(current_height + MaxProveCommitDuration::get() + SectorMaximumLifetime::get())
//             .build();

//         // Run pre commit extrinsic
//         assert_noop!(
//             StorageProvider::pre_commit_sector(
//                 RuntimeOrigin::signed(account(storage_provider)),
//                 sector.clone()
//             ),
//             Error::<Test>::MaxSectorLifetimeExceeded,
//         );
//     });
// }
