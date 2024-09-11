use frame_support::{assert_noop, assert_ok};
use sp_core::bounded_vec;

use super::{new_test_ext, MaxProveCommitDuration};
use crate::{
    pallet::{Error, Event, StorageProviders},
    sector::{ProveCommitResult, ProveCommitSector},
    storage_provider::StorageProviderError,
    tests::{
        account, events, publish_deals, register_storage_provider, run_to_block, Balances,
        RuntimeEvent, RuntimeOrigin, SectorPreCommitInfoBuilder, StorageProvider, System, Test,
        ALICE, BOB, CHARLIE, INITIAL_FUNDS,
    },
};

#[test]
fn successfully_prove_sector() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = CHARLIE;

        // Register storage provider
        register_storage_provider(account(storage_provider));
        // Set-up dependencies in Market Pallet
        publish_deals(storage_provider);

        // Sector to be pre-committed and proven
        let sector_number = 1;

        // Sector data
        let sector = SectorPreCommitInfoBuilder::default()
            .sector_number(sector_number)
            .build();

        // Run pre commit extrinsic
        assert_ok!(StorageProvider::pre_commit_sectors(
            RuntimeOrigin::signed(account(storage_provider)),
            bounded_vec![sector.clone()]
        ));

        // Remove any events that were triggered until now.
        System::reset_events();

        // Test prove commits
        let sector = ProveCommitSector {
            sector_number,
            proof: bounded_vec![0xd, 0xe, 0xa, 0xd],
        };

        assert_ok!(StorageProvider::prove_commit_sectors(
            RuntimeOrigin::signed(account(storage_provider)),
            bounded_vec![sector]
        ));
        assert_eq!(
            events(),
            [
                RuntimeEvent::Market(pallet_market::Event::DealActivated {
                    deal_id: 0,
                    client: account(ALICE),
                    provider: account(storage_provider)
                }),
                RuntimeEvent::Market(pallet_market::Event::DealActivated {
                    deal_id: 1,
                    client: account(BOB),
                    provider: account(storage_provider)
                }),
                RuntimeEvent::StorageProvider(Event::<Test>::SectorsProven {
                    owner: account(storage_provider),
                    sectors: bounded_vec![ProveCommitResult {
                        sector_number,
                        deadline_idx: 0,
                        partition_number: 0,
                    }]
                })
            ]
        );

        // check that the funds are still locked
        assert_eq!(
            Balances::free_balance(account(storage_provider)),
            // Provider reserved 70 tokens in the market pallet and 1 token is used for the pre-commit
            INITIAL_FUNDS - 70 - 1
        );
        let sp_state = StorageProviders::<Test>::get(account(storage_provider))
            .expect("Should be able to get providers info");

        // check that the sector has been activated
        assert!(!sp_state.sectors.is_empty());
        assert!(sp_state.sectors.contains_key(&sector_number));
        // always assigns first deadline and first partition, probably will fail when we change deadline calculation algo.
        let deadline = &sp_state.deadlines.due[0];
        let assigned_partition = &deadline.partitions[&0];
        assert_eq!(assigned_partition.sectors.len(), 1);
    });
}

#[test]
fn fails_storage_provider_not_found() {
    new_test_ext().execute_with(|| {
        // Test prove commits
        let sector = ProveCommitSector {
            sector_number: 1,
            proof: bounded_vec![0xd, 0xe, 0xa, 0xd],
        };

        assert_noop!(
            StorageProvider::prove_commit_sectors(
                RuntimeOrigin::signed(account(ALICE)),
                bounded_vec![sector]
            ),
            Error::<Test>::StorageProviderNotFound,
        );
    });
}

#[test]
fn fails_storage_precommit_missing() {
    new_test_ext().execute_with(|| {
        let storage_provider = ALICE;
        let sector_number = 1;

        // Register storage provider
        register_storage_provider(account(storage_provider));

        // Test prove commits
        let sector = ProveCommitSector {
            sector_number,
            proof: bounded_vec![0xd, 0xe, 0xa, 0xd],
        };

        assert_noop!(
            StorageProvider::prove_commit_sectors(
                RuntimeOrigin::signed(account(storage_provider)),
                bounded_vec![sector]
            ),
            Error::<Test>::StorageProviderError(StorageProviderError::SectorNotFound),
        );
    });
}

#[test]
fn fails_prove_commit_after_deadline() {
    // Block number at which the precommit is made
    let precommit_at_block_number = 1;
    // Block number at which the prove commit is made.
    let proving_at_block_number = precommit_at_block_number + MaxProveCommitDuration::get();

    new_test_ext().execute_with(|| {
        run_to_block(precommit_at_block_number);

        let storage_provider = CHARLIE;
        let sector_number = 1;

        // Register storage provider
        register_storage_provider(account(storage_provider));
        // Set-up dependencies in Market pallet
        publish_deals(storage_provider);

        // Sector data
        let sector = SectorPreCommitInfoBuilder::default()
            .sector_number(sector_number)
            .build();

        // Run pre commit extrinsic
        assert_ok!(StorageProvider::pre_commit_sectors(
            RuntimeOrigin::signed(account(storage_provider)),
            bounded_vec![sector.clone()]
        ));

        // Test prove commits
        let sector = ProveCommitSector {
            sector_number,
            proof: bounded_vec![0xd, 0xe, 0xa, 0xd],
        };

        run_to_block(proving_at_block_number);

        assert_noop!(
            StorageProvider::prove_commit_sectors(
                RuntimeOrigin::signed(account(storage_provider)),
                bounded_vec![sector]
            ),
            Error::<Test>::ProveCommitAfterDeadline,
        );
    });
}
