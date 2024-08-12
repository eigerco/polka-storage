use frame_support::{assert_noop, assert_ok};
use sp_core::bounded_vec;
use sp_runtime::{BoundedVec, DispatchError};

use super::new_test_ext;
use crate::{
    pallet::{Error, Event, StorageProviders},
    sector::MAX_SECTORS,
    tests::{
        account, cid_of, events, publish_deals, register_storage_provider, run_to_block, Balances,
        MaxProveCommitDuration, MaxSectorExpirationExtension, RuntimeEvent, RuntimeOrigin,
        SectorPreCommitInfoBuilder, StorageProvider, Test, ALICE, CHARLIE, INITIAL_FUNDS,
    },
};

#[test]
fn successfully_precommited() {
    new_test_ext().execute_with(|| {
        // Register CHARLIE as a storage provider.
        let storage_provider = CHARLIE;
        register_storage_provider(account(storage_provider));
        // Publish deals for verification before pre-commit.
        publish_deals(storage_provider);

        // Sector to be pre-committed.
        let sector = SectorPreCommitInfoBuilder::default().build();

        // Check starting balance
        assert_eq!(
            Balances::free_balance(account(storage_provider)),
            INITIAL_FUNDS - 70
        );

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

        let sp = StorageProviders::<Test>::get(account(storage_provider))
            .expect("SP should be present because of the pre-check");

        assert!(sp.sectors.is_empty()); // not yet proven
        assert!(!sp.pre_committed_sectors.is_empty());
        assert_eq!(sp.pre_commit_deposits, 1);
        assert_eq!(
            Balances::free_balance(account(storage_provider)),
            INITIAL_FUNDS - 70 - 1  // 1 for pre-commit deposit
        );
    });
}

#[test]
fn successfully_precommited_no_deals() {
    new_test_ext().execute_with(|| {
        // Register CHARLIE as a storage provider.
        let storage_provider = CHARLIE;
        register_storage_provider(account(storage_provider));

        // Sector to be pre-committed.
        let sector = SectorPreCommitInfoBuilder::default()
            // No sectors -> No CommD verification
            .deals(bounded_vec![])
            .unsealed_cid(
                cid_of("cc-unsealed-cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
            )
            .build();

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
                    sector,
                })
            ]
        );

        let sp = StorageProviders::<Test>::get(account(storage_provider))
            .expect("SP should be present because of the pre-check");

        assert!(sp.sectors.is_empty()); // not yet proven
        assert!(!sp.pre_committed_sectors.is_empty());
        assert_eq!(sp.pre_commit_deposits, 1);

        assert_eq!(
            Balances::free_balance(account(storage_provider)),
            INITIAL_FUNDS - 1
        );
    });
}

#[test]
fn fails_should_be_signed() {
    new_test_ext().execute_with(|| {
        // Sector to be pre-committed
        let sector = SectorPreCommitInfoBuilder::default().build();

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
        let sector = SectorPreCommitInfoBuilder::default().build();

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
        // Register CHARLIE as a storage provider.
        let storage_provider = CHARLIE;
        register_storage_provider(account(storage_provider));
        publish_deals(storage_provider);

        // Sector to be pre-committed
        let sector = SectorPreCommitInfoBuilder::default().build();

        // Run pre commit extrinsic
        assert_ok!(StorageProvider::pre_commit_sector(
            RuntimeOrigin::signed(account(storage_provider)),
            sector.clone()
        ));
        // Run same extrinsic, this should fail
        assert_noop!(
            StorageProvider::pre_commit_sector(
                RuntimeOrigin::signed(account(storage_provider)),
                sector
            ),
            Error::<Test>::SectorNumberAlreadyUsed,
        );
    });
}

#[test]
fn fails_declared_commd_not_matching() {
    new_test_ext().execute_with(|| {
        // Register CHARLIE as a storage provider.
        let storage_provider = CHARLIE;
        register_storage_provider(account(storage_provider));
        publish_deals(storage_provider);

        // Sector to be pre-committed
        let sector = SectorPreCommitInfoBuilder::default()
            .unsealed_cid(
                cid_of("different-unsealed-cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
            )
            .build();

        assert_noop!(
            StorageProvider::pre_commit_sector(
                RuntimeOrigin::signed(account(storage_provider)),
                sector
            ),
            Error::<Test>::InvalidUnsealedCidForSector,
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
            .sector_number(MAX_SECTORS as u64 + 1)
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
        let mut sector = SectorPreCommitInfoBuilder::default().build();

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
