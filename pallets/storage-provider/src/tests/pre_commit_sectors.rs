use frame_support::{assert_noop, assert_ok, pallet_prelude::*};
use frame_system::pallet_prelude::BlockNumberFor;
use primitives::proofs::MAX_SECTORS_PER_CALL;
use sp_core::bounded_vec;
use sp_runtime::{BoundedVec, DispatchError};

use super::new_test_ext;
use crate::{
    pallet::{Error, Event, StorageProviders},
    sector::SectorPreCommitInfo,
    tests::{
        account, events, publish_deals, register_storage_provider, run_to_block, Balances,
        MaxProveCommitDuration, MaxSectorExpiration, RuntimeEvent, RuntimeOrigin,
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
        let sector = SectorPreCommitInfoBuilder::default()
            .unsealed_cid("baga6ea4seaqhdbbdnon7gkuquzw6waekzqx5lbuio6a6wjie22pgfmwnv3a3wfi")
            .build();

        // Check starting balance
        assert_eq!(
            Balances::free_balance(account(storage_provider)),
            INITIAL_FUNDS - 70
        );

        // Run pre commit extrinsic
        assert_ok!(StorageProvider::pre_commit_sectors(
            RuntimeOrigin::signed(account(storage_provider)),
            bounded_vec![sector.clone()],
        ));

        // Check that the events were triggered
        assert_eq!(
            events(),
            [
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Reserved {
                    who: account(storage_provider),
                    amount: 1
                },),
                RuntimeEvent::StorageProvider(Event::<Test>::SectorsPreCommitted {
                    block: 1,
                    owner: account(storage_provider),
                    sectors: bounded_vec![sector],
                })
            ]
        );

        let sp = StorageProviders::<Test>::get(account(storage_provider))
            .expect("SP should be present because of the pre-check");

        assert!(sp.sectors.is_empty()); // not yet proven
        assert_eq!(sp.pre_committed_sectors.len(), 1);
        assert_eq!(sp.pre_commit_deposits, 1);
        assert_eq!(
            Balances::free_balance(account(storage_provider)),
            INITIAL_FUNDS - 70 - 1 // 1 for pre-commit deposit
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
            .build();

        // Run pre commit extrinsic
        assert_ok!(StorageProvider::pre_commit_sectors(
            RuntimeOrigin::signed(account(storage_provider)),
            bounded_vec![sector.clone()],
        ));

        // Check that the events were triggered
        assert_eq!(
            events(),
            [
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Reserved {
                    who: account(storage_provider),
                    amount: 1
                },),
                RuntimeEvent::StorageProvider(Event::<Test>::SectorsPreCommitted {
                    block: 1,
                    owner: account(storage_provider),
                    sectors: bounded_vec![sector],
                })
            ]
        );

        let sp = StorageProviders::<Test>::get(account(storage_provider))
            .expect("SP should be present because of the pre-check");

        assert!(sp.sectors.is_empty()); // not yet proven
        assert_eq!(sp.pre_committed_sectors.len(), 1);
        assert_eq!(sp.pre_commit_deposits, 1);

        assert_eq!(
            Balances::free_balance(account(storage_provider)),
            INITIAL_FUNDS - 1
        );
    });
}

#[test]
fn successfully_precommited_batch() {
    new_test_ext().execute_with(|| {
        const SECTORS_TO_PRECOMMIT: u64 = 6;
        // Register CHARLIE as a storage provider.
        let storage_provider = CHARLIE;
        register_storage_provider(account(storage_provider));
        // Publish deals for verification before pre-commit.
        publish_deals(storage_provider);

        // Create 6 sectors in pre-commit
        let mut sectors: BoundedVec<
            SectorPreCommitInfo<BlockNumberFor<Test>>,
            ConstU32<MAX_SECTORS_PER_CALL>,
        > = bounded_vec![];
        for sector_number in 0..SECTORS_TO_PRECOMMIT {
            let sector_number = u16::try_from(sector_number).unwrap();
            sectors
                .try_push(
                    SectorPreCommitInfoBuilder::default()
                        .sector_number(sector_number.into())
                        .unsealed_cid(
                            "baga6ea4seaqhdbbdnon7gkuquzw6waekzqx5lbuio6a6wjie22pgfmwnv3a3wfi",
                        )
                        .build(),
                )
                .expect("BoundedVec should fit all 6 elements");
        }

        // Run pre commit extrinsic
        assert_ok!(StorageProvider::pre_commit_sectors(
            RuntimeOrigin::signed(account(storage_provider)),
            sectors.clone(),
        ));

        // Check that the events were triggered
        assert_eq!(
            events(),
            [
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Reserved {
                    who: account(storage_provider),
                    amount: SECTORS_TO_PRECOMMIT
                },),
                RuntimeEvent::StorageProvider(Event::<Test>::SectorsPreCommitted {
                    block: 1,
                    owner: account(storage_provider),
                    sectors,
                })
            ]
        );

        let sp = StorageProviders::<Test>::get(account(storage_provider))
            .expect("SP should be present because of the pre-check");

        assert!(sp.sectors.is_empty()); // not yet proven
        assert_eq!(
            sp.pre_committed_sectors.len(),
            (SECTORS_TO_PRECOMMIT as usize)
        );
        assert_eq!(sp.pre_commit_deposits, SECTORS_TO_PRECOMMIT);
        assert_eq!(
            Balances::free_balance(account(storage_provider)),
            INITIAL_FUNDS - 70 - SECTORS_TO_PRECOMMIT
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
            StorageProvider::pre_commit_sectors(RuntimeOrigin::none(), bounded_vec![sector]),
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
            StorageProvider::pre_commit_sectors(
                RuntimeOrigin::signed(account(ALICE)),
                bounded_vec![sector]
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
        let sector = SectorPreCommitInfoBuilder::default()
            .unsealed_cid("baga6ea4seaqhdbbdnon7gkuquzw6waekzqx5lbuio6a6wjie22pgfmwnv3a3wfi")
            .build();

        // Run pre commit extrinsic
        assert_ok!(StorageProvider::pre_commit_sectors(
            RuntimeOrigin::signed(account(storage_provider)),
            bounded_vec![sector.clone()]
        ));
        // Run same extrinsic, this should fail
        assert_noop!(
            StorageProvider::pre_commit_sectors(
                RuntimeOrigin::signed(account(storage_provider)),
                bounded_vec![sector]
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
            // wrong cid for for the sector
            .unsealed_cid("baga6ea4seaqmruupwrxaeck7m3f5jtswpr7jv6bvwqeu5jinzjlcybh6er3ficq")
            .build();

        assert_noop!(
            StorageProvider::pre_commit_sectors(
                RuntimeOrigin::signed(account(storage_provider)),
                bounded_vec![sector]
            ),
            Error::<Test>::InvalidUnsealedCidForSector,
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
            StorageProvider::pre_commit_sectors(
                RuntimeOrigin::signed(account(storage_provider)),
                bounded_vec![sector]
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
            StorageProvider::pre_commit_sectors(
                RuntimeOrigin::signed(account(storage_provider)),
                bounded_vec![sector]
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
            StorageProvider::pre_commit_sectors(
                RuntimeOrigin::signed(account(storage_provider)),
                bounded_vec![sector]
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
            .expiration(current_height + MaxSectorExpiration::get() + 1)
            .build();

        // Run pre commit extrinsic
        assert_noop!(
            StorageProvider::pre_commit_sectors(
                RuntimeOrigin::signed(account(storage_provider)),
                bounded_vec![sector]
            ),
            Error::<Test>::ExpirationTooLong,
        );
    });
}
