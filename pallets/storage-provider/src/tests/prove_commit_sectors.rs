use frame_support::{assert_noop, assert_ok, pallet_prelude::*};
use frame_system::pallet_prelude::BlockNumberFor;
use primitives_proofs::MAX_SECTORS_PER_CALL;
use sp_core::bounded_vec;

use super::{new_test_ext, MaxProveCommitDuration};
use crate::{
    deadline::deadline_is_mutable,
    error::GeneralPalletError,
    pallet::{Error, Event, StorageProviders},
    sector::{ProveCommitResult, ProveCommitSector, SectorPreCommitInfo},
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
            .unsealed_cid("baga6ea4seaqhdbbdnon7gkuquzw6waekzqx5lbuio6a6wjie22pgfmwnv3a3wfi")
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

        // Run to the block, where we will be able to prove commit the sector.
        run_to_block(4);

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
fn successfully_prove_multiple_sectors() {
    new_test_ext().execute_with(|| {
        const SECTORS_TO_COMMIT: u64 = 2;
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
        for sector_number in 0..SECTORS_TO_COMMIT {
            sectors
                .try_push(
                    SectorPreCommitInfoBuilder::default()
                        .sector_number(sector_number)
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

        // Remove any events that were triggered until now.
        System::reset_events();

        // Run to the block where we can prove commit the sector.
        run_to_block(System::block_number() + 2);

        // Create 6 prove commits and the expected result
        let mut sectors: BoundedVec<ProveCommitSector, ConstU32<MAX_SECTORS_PER_CALL>> =
            bounded_vec![];
        let mut expected_sector_results: BoundedVec<
            ProveCommitResult,
            ConstU32<MAX_SECTORS_PER_CALL>,
        > = bounded_vec![];
        for sector_number in 0..SECTORS_TO_COMMIT {
            sectors
                .try_push(ProveCommitSector {
                    sector_number,
                    proof: bounded_vec![0xd, 0xe, 0xa, 0xd],
                })
                .expect("BoundedVec should fit all 6 elements");
            expected_sector_results
                .try_push(ProveCommitResult {
                    sector_number,
                    deadline_idx: 0, // due is grouped by partition so 2 elements will be at deadline_idx 0
                    partition_number: 0,
                })
                .expect("BoundedVec should fit all 6 elements");
        }

        assert_ok!(StorageProvider::prove_commit_sectors(
            RuntimeOrigin::signed(account(storage_provider)),
            sectors,
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
                    sectors: expected_sector_results
                })
            ]
        );

        // check that the funds are still locked
        assert_eq!(
            Balances::free_balance(account(storage_provider)),
            // Provider reserved 70 tokens in the market pallet and 1 token is used per the pre-commit
            INITIAL_FUNDS - 70 - SECTORS_TO_COMMIT
        );
        let sp_state = StorageProviders::<Test>::get(account(storage_provider))
            .expect("Should be able to get providers info");

        // check that the sector has been activated
        assert!(!sp_state.sectors.is_empty());
        for sector_number in 0..SECTORS_TO_COMMIT {
            assert!(sp_state.sectors.contains_key(&sector_number));
        }
        // always assigns first deadline and first partition, probably will fail when we change deadline calculation algo.
        let deadline = &sp_state.deadlines.due[0];
        let assigned_partition = &deadline.partitions[&0];
        assert_eq!(
            assigned_partition.sectors.len(),
            (SECTORS_TO_COMMIT as usize)
        );
    });
}

#[test]
fn successfully_prove_after_period_start_and_check_mutability() {
    new_test_ext().execute_with(|| {
        const SECTORS_TO_COMMIT: u64 = 4;
        // Register CHARLIE as a storage provider.
        let storage_provider = CHARLIE;
        register_storage_provider(account(storage_provider));
        // Publish deals for verification before pre-commit.
        // For this test we don't really care if the deal is activated or not
        // We just need a deal in the sector so we don't error.
        // Other deals not found will just be skipped.
        publish_deals(storage_provider);
        // Run to block after period start (61)
        run_to_block(69);

        // Create sectors in pre-commit
        let mut sectors: BoundedVec<
            SectorPreCommitInfo<BlockNumberFor<Test>>,
            ConstU32<MAX_SECTORS_PER_CALL>,
        > = bounded_vec![];
        for sector_number in 0..SECTORS_TO_COMMIT {
            sectors
                .try_push(
                    SectorPreCommitInfoBuilder::default()
                        .sector_number(sector_number)
                        .unsealed_cid(
                            "baga6ea4seaqhdbbdnon7gkuquzw6waekzqx5lbuio6a6wjie22pgfmwnv3a3wfi",
                        )
                        .expiration(170) // Max lifetime = 120 blocks
                        .build(),
                )
                .expect("BoundedVec should fit all elements");
        }

        // Run pre commit extrinsic
        assert_ok!(StorageProvider::pre_commit_sectors(
            RuntimeOrigin::signed(account(storage_provider)),
            sectors.clone(),
        ));

        // Remove any events that were triggered until now.
        System::reset_events();

        // Run to the block right before close (5 blocks after prove commit)
        run_to_block(73);

        // Check the deadlines in due
        // We are expecting deadlines index 3 and 4 to be immutable
        // Because they close at 71 and 75 respectively with a challenge window of 4
        // meaning that the mutability of these deadlines closes at 67 and 71 respectively.
        // The reason index 0-2 are mutable is because their closing has passed and a new open/close has been set.
        // Check deadline's fn new() for the logic on open/close calculation
        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();
        for (idx, _dl) in sp.deadlines.due.iter().enumerate() {
            let is_mutable = deadline_is_mutable(
                sp.proving_period_start,
                idx as u64,
                System::block_number(),
                <Test as crate::Config>::WPoStPeriodDeadlines::get(),
                <Test as crate::Config>::WPoStProvingPeriod::get(),
                <Test as crate::Config>::WPoStChallengeWindow::get(),
                <Test as crate::Config>::WPoStChallengeLookBack::get(),
                <Test as crate::Config>::FaultDeclarationCutoff::get(),
            )
            .unwrap();

            if idx == 2 || idx == 3 {
                assert!(!is_mutable);
            } else {
                assert!(is_mutable);
            }
        }
        let mut sectors: BoundedVec<ProveCommitSector, ConstU32<MAX_SECTORS_PER_CALL>> =
            bounded_vec![];
        let mut expected_sector_results: BoundedVec<
            ProveCommitResult,
            ConstU32<MAX_SECTORS_PER_CALL>,
        > = bounded_vec![];
        for sector_number in 0..SECTORS_TO_COMMIT {
            sectors
                .try_push(ProveCommitSector {
                    sector_number,
                    proof: bounded_vec![0xd, 0xe, 0xa, 0xd],
                })
                .expect("BoundedVec should fit all elements");
            expected_sector_results
                .try_push(ProveCommitResult {
                    sector_number,
                    deadline_idx: 0, // due is grouped by partition so all elements will be at deadline_idx 0
                    partition_number: 0,
                })
                .expect("BoundedVec should fit all elements");
        }

        assert_ok!(StorageProvider::prove_commit_sectors(
            RuntimeOrigin::signed(account(storage_provider)),
            sectors,
        ));
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
            Error::<Test>::GeneralPalletError(
                GeneralPalletError::StorageProviderErrorSectorNotFound
            ),
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
            .unsealed_cid("baga6ea4seaqhdbbdnon7gkuquzw6waekzqx5lbuio6a6wjie22pgfmwnv3a3wfi")
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
