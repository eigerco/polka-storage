use codec::Encode;
use frame_support::{
    assert_noop, assert_ok,
    sp_runtime::{bounded_vec, BoundedVec},
    traits::ConstU32,
};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_market::{BalanceOf, ClientDealProposal, DealProposal, DealState};
use primitives_proofs::{RegisteredPoStProof, RegisteredSealProof};
use sp_core::Pair;
use sp_runtime::MultiSignature;

use crate::{
    mock::*,
    pallet::{Error, Event, StorageProviders},
    sector::{ProveCommitSector, SectorPreCommitInfo},
    storage_provider::StorageProviderInfo,
};

#[test]
fn initial_state() {
    new_test_ext(1).execute_with(|| {
        assert!(!StorageProviders::<Test>::contains_key(account(ALICE)));
        assert!(!StorageProviders::<Test>::contains_key(account(BOB)));
    })
}

mod storage_provider_registration {
    use sp_runtime::DispatchError;

    use super::*;

    /// Tests if storage provider registration is successful.
    #[test]
    fn successful_registration() {
        new_test_ext(1).execute_with(|| {
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
                    },
                )]
            );
        })
    }

    #[test]
    fn fails_should_be_signed() {
        new_test_ext(1).execute_with(|| {
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
        new_test_ext(1).execute_with(|| {
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
}

mod pre_commit_sector {
    use sp_runtime::DispatchError;

    use crate::sector::SECTORS_MAX;

    use super::*;

    #[test]
    fn successfully_precommited() {
        new_test_ext(1).execute_with(|| {
            // Register ALICE as a storage provider.
            let account = account(ALICE);
            register_storage_provider(account.clone());

            // Sector to be pre-committed.
            let sector = SectorPreCommitInfo {
                seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
                sector_number: 1,
                sealed_cid: cid_of("sealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                deal_ids: bounded_vec![0, 1],
                expiration: YEARS,
                unsealed_cid: cid_of("unsealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
            };

            // Check starting balance
            assert_eq!(Balances::free_balance(account.clone()), 100);
            // Run pre commit extrinsic
            StorageProvider::pre_commit_sector(
                RuntimeOrigin::signed(account.clone()),
                sector.clone(),
            )
            .expect("Pre commit failed");
            // Check that the event triggered
            assert_eq!(
                events(),
                [
                    RuntimeEvent::Balances(pallet_balances::Event::<Test>::Reserved {
                        who: account.clone(),
                        amount: 1
                    },),
                    RuntimeEvent::StorageProvider(Event::<Test>::SectorPreCommitted {
                        owner: account.clone(),
                        sector: sector.clone(),
                    })
                ]
            );
            let sp_alice = StorageProviders::<Test>::get(account.clone())
                .expect("SP Alice should be present because of the pre-check");

            assert!(sp_alice.sectors.is_empty()); // not yet proven
            assert!(!sp_alice.pre_committed_sectors.is_empty());
            assert_eq!(sp_alice.pre_commit_deposits, 1);
            assert_eq!(Balances::free_balance(account), 99);
        });
    }

    #[test]
    fn fails_should_be_signed() {
        new_test_ext(1).execute_with(|| {
            // Sector to be pre-committed
            let sector = SectorPreCommitInfo {
                seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
                sector_number: SECTORS_MAX as u64 + 1,
                sealed_cid: cid_of("sealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                deal_ids: bounded_vec![0, 1],
                expiration: YEARS,
                unsealed_cid: cid_of("unsealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
            };

            // Run pre commit extrinsic
            assert_noop!(
                StorageProvider::pre_commit_sector(RuntimeOrigin::none(), sector.clone()),
                DispatchError::BadOrigin,
            );
        });
    }

    #[test]
    fn fails_storage_provider_not_found() {
        new_test_ext(1).execute_with(|| {
            // Sector to be pre-committed
            let sector = SectorPreCommitInfo {
                seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
                sector_number: 1,
                sealed_cid: cid_of("sealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                deal_ids: bounded_vec![0, 1],
                expiration: YEARS,
                unsealed_cid: cid_of("unsealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
            };

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
        new_test_ext(1).execute_with(|| {
            // Register ALICE as a storage provider.
            let account = account(ALICE);
            register_storage_provider(account.clone());

            // Sector to be pre-committed
            let sector = SectorPreCommitInfo {
                seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
                sector_number: 1,
                sealed_cid: cid_of("sealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                deal_ids: bounded_vec![0, 1],
                expiration: YEARS,
                unsealed_cid: cid_of("unsealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
            };
            // Run pre commit extrinsic
            assert_ok!(StorageProvider::pre_commit_sector(
                RuntimeOrigin::signed(account.clone()),
                sector.clone()
            ));
            // Run same extrinsic, this should fail
            assert_noop!(
                StorageProvider::pre_commit_sector(
                    RuntimeOrigin::signed(account.clone()),
                    sector.clone()
                ),
                Error::<Test>::SectorNumberAlreadyUsed,
            );
        });
    }

    #[test]
    fn fails_invalid_sector() {
        new_test_ext(1).execute_with(|| {
            // Register ALICE as a storage provider.
            let account = account(ALICE);
            register_storage_provider(account.clone());

            // Sector to be pre-committed
            let sector = SectorPreCommitInfo {
                seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
                sector_number: SECTORS_MAX as u64 + 1,
                sealed_cid: cid_of("sealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                deal_ids: bounded_vec![0, 1],
                expiration: YEARS,
                unsealed_cid: cid_of("unsealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
            };

            // Run pre commit extrinsic
            assert_noop!(
                StorageProvider::pre_commit_sector(RuntimeOrigin::signed(account), sector.clone()),
                Error::<Test>::InvalidSector,
            );
        });
    }

    #[test]
    fn fails_invalid_cid() {
        new_test_ext(1).execute_with(|| {
            // Register ALICE as a storage provider.
            let account = account(ALICE);
            register_storage_provider(account.clone());

            // Sector to be pre-committed
            let sector = SectorPreCommitInfo {
                seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
                sector_number: 1,
                sealed_cid: cid_of("sealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                deal_ids: bounded_vec![0, 1],
                expiration: YEARS,
                // Wrong cid set
                unsealed_cid: BoundedVec::new(),
            };

            // Run pre commit extrinsic
            assert_noop!(
                StorageProvider::pre_commit_sector(RuntimeOrigin::signed(account), sector.clone()),
                Error::<Test>::InvalidCid,
            );
        });
    }

    #[test]
    fn fails_expiration_before_activation() {
        new_test_ext(1000).execute_with(|| {
            // Register ALICE as a storage provider.
            let account = account(ALICE);
            register_storage_provider(account.clone());

            // Sector to be pre-committed
            let sector = SectorPreCommitInfo {
                seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
                sector_number: 1,
                sealed_cid: cid_of("sealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                deal_ids: bounded_vec![0, 1],
                expiration: 1000,
                unsealed_cid: cid_of("unsealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
            };

            // Run pre commit extrinsic
            assert_noop!(
                StorageProvider::pre_commit_sector(RuntimeOrigin::signed(account), sector.clone()),
                Error::<Test>::ExpirationBeforeActivation,
            );
        });
    }

    #[test]
    fn fails_expiration_too_soon() {
        let current_height = 1000;

        new_test_ext(current_height).execute_with(|| {
            // Register ALICE as a storage provider.
            let account = account(ALICE);
            register_storage_provider(account.clone());

            // Sector to be pre-committed
            let sector = SectorPreCommitInfo {
                seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
                sector_number: 1,
                sealed_cid: cid_of("sealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                deal_ids: bounded_vec![0, 1],
                // Set expiration to be in the next block after the maximum
                // allowed activation.
                expiration: current_height + MaxProveCommitDuration::get() + 1,
                unsealed_cid: cid_of("unsealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
            };

            // Run pre commit extrinsic
            assert_noop!(
                StorageProvider::pre_commit_sector(RuntimeOrigin::signed(account), sector.clone()),
                Error::<Test>::ExpirationTooSoon,
            );
        });
    }

    #[test]
    fn fails_expiration_too_long() {
        let current_height = 1000;

        new_test_ext(current_height).execute_with(|| {
            // Register ALICE as a storage provider.
            let account = account(ALICE);
            register_storage_provider(account.clone());

            // Sector to be pre-committed
            let sector = SectorPreCommitInfo {
                seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
                sector_number: 1,
                sealed_cid: cid_of("sealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                deal_ids: bounded_vec![0, 1],
                // Set expiration to be in the next block after the maximum
                // allowed
                expiration: current_height + MaxSectorExpirationExtension::get() + 1,
                unsealed_cid: cid_of("unsealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
            };

            // Run pre commit extrinsic
            assert_noop!(
                StorageProvider::pre_commit_sector(RuntimeOrigin::signed(account), sector.clone()),
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

    //     new_test_ext(current_height).execute_with(|| {
    //         // Register ALICE as a storage provider.
    //         let account = account(ALICE);
    //         register_storage_provider(account.clone());

    //         // Sector to be pre-committed
    //         let sector = SectorPreCommitInfo {
    //             seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
    //             sector_number: 1,
    //             sealed_cid: cid_of("sealed_cid")
    //                 .to_bytes()
    //                 .try_into()
    //                 .expect("hash is always 32 bytes"),
    //             deal_ids: bounded_vec![0, 1],
    //             // Set expiration to be in the next block after the maximum
    //             // allowed
    //             expiration: current_height + MaxProveCommitDuration::get() + SectorMaximumLifetime::get(),
    //             unsealed_cid: cid_of("unsealed_cid")
    //                 .to_bytes()
    //                 .try_into()
    //                 .expect("hash is always 32 bytes"),
    //         };

    //         // Run pre commit extrinsic
    //         assert_noop!(
    //             StorageProvider::pre_commit_sector(
    //                 RuntimeOrigin::signed(account),
    //                 sector.clone()
    //             ),
    //             Error::<Test>::MaxSectorLifetimeExceeded,
    //         );
    //     });
    // }
}

mod prove_commit_sector {
    use sp_runtime::DispatchError;

    use crate::sector::SECTORS_MAX;

    use super::*;

    #[test]
    fn successfully_prove_sector() {
        new_test_ext(1).execute_with(|| {
            // Setup accounts
            let storage_provider = ALICE;
            let storage_client = BOB;

            // Register storage provider
            register_storage_provider(account(storage_provider));

            // Add balance to the market pallet
            let _ = Market::add_balance(RuntimeOrigin::signed(account(storage_provider)), 60);
            let _ = Market::add_balance(RuntimeOrigin::signed(account(storage_client)), 70);

            // Generate a deal proposal
            let deal_proposal = DealProposalBuilder::default()
                .client(storage_client)
                .provider(storage_provider)
                .signed(storage_client);

            // Publish the deal proposal
            assert_ok!(Market::publish_storage_deals(
                RuntimeOrigin::signed(account(storage_provider)),
                bounded_vec![deal_proposal],
            ));

            // Sector to be pre-committed and proven
            let sector_number = 1;

            // Sector data
            let sector = SectorPreCommitInfo {
                seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
                sector_number,
                sealed_cid: cid_of("sealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                deal_ids: bounded_vec![0],
                expiration: YEARS,
                unsealed_cid: cid_of("unsealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
            };

            // Run pre commit extrinsic
            assert_ok!(StorageProvider::pre_commit_sector(
                RuntimeOrigin::signed(account(storage_provider)),
                sector.clone()
            ));

            // flush the events
            events();

            // Test prove commits
            let sector = ProveCommitSector {
                sector_number,
                proof: cid_of("prove_commit")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
            };

            assert_ok!(StorageProvider::prove_commit_sector(
                RuntimeOrigin::signed(account(storage_provider)),
                sector
            ));
            assert_eq!(
                events(),
                [
                    RuntimeEvent::Market(pallet_market::Event::DealActivated {
                        deal_id: 0,
                        client: account(storage_client),
                        provider: account(storage_provider)
                    }),
                    RuntimeEvent::StorageProvider(Event::<Test>::SectorProven {
                        owner: account(storage_provider),
                        sector_number: sector_number
                    })
                ]
            );

            // check that the funds are still locked
            assert_eq!(Balances::free_balance(account(storage_provider)), 39);
            let sp_state = StorageProviders::<Test>::get(account(storage_provider))
                .expect("Should be able to get providers info");
            // check that the sector has been activated
            assert!(!sp_state.sectors.is_empty());
            assert!(sp_state.sectors.contains_key(&sector_number));
        });
    }

    #[test]
    fn fails_should_be_signed() {
        new_test_ext(1).execute_with(|| {
            // Sector to be pre-committed
            let sector = SectorPreCommitInfo {
                seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
                sector_number: SECTORS_MAX as u64 + 1,
                sealed_cid: cid_of("sealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                deal_ids: bounded_vec![0, 1],
                expiration: YEARS,
                unsealed_cid: cid_of("unsealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
            };

            // Run pre commit extrinsic
            assert_noop!(
                StorageProvider::pre_commit_sector(RuntimeOrigin::none(), sector.clone()),
                DispatchError::BadOrigin,
            );
        });
    }

    #[test]
    fn fails_storage_provider_not_found() {
        new_test_ext(1).execute_with(|| {
            let storage_provider = ALICE;
            let storage_client = BOB;
            let sector_number = 1;

            // Register storage provider
            register_storage_provider(account(storage_provider));

            // Add balance to the market pallet
            let _ = Market::add_balance(RuntimeOrigin::signed(account(storage_provider)), 60);
            let _ = Market::add_balance(RuntimeOrigin::signed(account(storage_client)), 70);

            // Generate a deal proposal
            let deal_proposal = DealProposalBuilder::default()
                .client(storage_client)
                .provider(storage_provider)
                .signed(storage_client);

            // Publish the deal proposal
            assert_ok!(Market::publish_storage_deals(
                RuntimeOrigin::signed(account(storage_provider)),
                bounded_vec![deal_proposal],
            ));

            // Sector data
            let sector = SectorPreCommitInfo {
                seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
                sector_number,
                sealed_cid: cid_of("sealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                deal_ids: bounded_vec![0],
                expiration: YEARS,
                unsealed_cid: cid_of("unsealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
            };

            // Run pre commit extrinsic
            assert_ok!(StorageProvider::pre_commit_sector(
                RuntimeOrigin::signed(account(storage_provider)),
                sector.clone()
            ));

            // Test prove commits
            let sector = ProveCommitSector {
                sector_number,
                proof: cid_of("prove_commit")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
            };

            assert_noop!(
                StorageProvider::prove_commit_sector(
                    RuntimeOrigin::signed(account(CHARLIE)),
                    sector
                ),
                Error::<Test>::StorageProviderNotFound,
            );
        });
    }

    #[test]
    fn fails_storage_precommit_missing() {
        new_test_ext(1).execute_with(|| {
            let storage_provider = ALICE;
            let sector_number = 1;

            // Register storage provider
            register_storage_provider(account(storage_provider));

            // Test prove commits
            let sector = ProveCommitSector {
                sector_number,
                proof: cid_of("prove_commit")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
            };

            assert_noop!(
                StorageProvider::prove_commit_sector(
                    RuntimeOrigin::signed(account(storage_provider)),
                    sector
                ),
                Error::<Test>::InvalidSector,
            );
        });
    }

    #[test]
    fn fails_prove_commit_after_deadline() {
        // Block number at which the precommit is made
        let precommit_at_block_number = 1;
        // Block number at which the prove commit is made.
        let proving_at_block_number = precommit_at_block_number + MaxProveCommitDuration::get();

        new_test_ext(precommit_at_block_number).execute_with(|| {
            let storage_provider = ALICE;
            let storage_client = BOB;
            let sector_number = 1;

            // Register storage provider
            register_storage_provider(account(storage_provider));

            // Add balance to the market pallet
            let _ = Market::add_balance(RuntimeOrigin::signed(account(storage_provider)), 60);
            let _ = Market::add_balance(RuntimeOrigin::signed(account(storage_client)), 70);

            // Generate a deal proposal
            let deal_proposal = DealProposalBuilder::default()
                .client(storage_client)
                .provider(storage_provider)
                .signed(storage_client);

            // Publish the deal proposal
            assert_ok!(Market::publish_storage_deals(
                RuntimeOrigin::signed(account(storage_provider)),
                bounded_vec![deal_proposal],
            ));

            // Sector data
            let sector = SectorPreCommitInfo {
                seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
                sector_number,
                sealed_cid: cid_of("sealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                deal_ids: bounded_vec![0],
                expiration: YEARS,
                unsealed_cid: cid_of("unsealed_cid")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
            };

            // Run pre commit extrinsic
            assert_ok!(StorageProvider::pre_commit_sector(
                RuntimeOrigin::signed(account(storage_provider)),
                sector.clone()
            ));

            // Test prove commits
            let sector = ProveCommitSector {
                sector_number,
                proof: cid_of("prove_commit")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
            };

            System::set_block_number(proving_at_block_number);
            assert_noop!(
                StorageProvider::prove_commit_sector(
                    RuntimeOrigin::signed(account(storage_provider)),
                    sector
                ),
                Error::<Test>::ProveCommitAfterDeadline,
            );
        });
    }
}

/// Register account as a provider.
fn register_storage_provider(account: AccountIdOf<Test>) {
    let peer_id = "storage_provider_1".as_bytes().to_vec();
    let peer_id = BoundedVec::try_from(peer_id).unwrap();
    let window_post_type = RegisteredPoStProof::StackedDRGWindow2KiBV1P1;

    // Register account as a storage provider.
    let _ = StorageProvider::register_storage_provider(
        RuntimeOrigin::signed(account),
        peer_id.clone(),
        window_post_type,
    );

    // Remove any events that were triggered during registration.
    System::reset_events();
}

/// Builder to simplify writing complex tests of [`DealProposal`].
/// Exclusively uses [`Test`] for simplification purposes.
struct DealProposalBuilder {
    piece_cid: BoundedVec<u8, ConstU32<128>>,
    piece_size: u64,
    client: AccountIdOf<Test>,
    provider: AccountIdOf<Test>,
    label: BoundedVec<u8, ConstU32<128>>,
    start_block: u64,
    end_block: u64,
    storage_price_per_block: u64,
    provider_collateral: u64,
    state: DealState<u64>,
}

impl Default for DealProposalBuilder {
    fn default() -> Self {
        Self {
            piece_cid: cid_of("polka-storage-data")
                .to_bytes()
                .try_into()
                .expect("hash is always 32 bytes"),
            piece_size: 18,
            client: account(BOB),
            provider: account(ALICE),
            label: bounded_vec![0xb, 0xe, 0xe, 0xf],
            start_block: 100,
            end_block: 110,
            storage_price_per_block: 5,
            provider_collateral: 25,
            state: DealState::Published,
        }
    }
}

impl DealProposalBuilder {
    pub fn client(mut self, client: &str) -> Self {
        self.client = account(client);
        self
    }

    pub fn provider(mut self, provider: &str) -> Self {
        self.provider = account(provider);
        self
    }

    pub fn unsigned(self) -> DealProposalOf<Test> {
        DealProposalOf::<Test> {
            piece_cid: self.piece_cid,
            piece_size: self.piece_size,
            client: self.client,
            provider: self.provider,
            label: self.label,
            start_block: self.start_block,
            end_block: self.end_block,
            storage_price_per_block: self.storage_price_per_block,
            provider_collateral: self.provider_collateral,
            state: self.state,
        }
    }

    pub fn signed(self, by: &str) -> ClientDealProposalOf<Test> {
        let built = self.unsigned();
        let signed = sign_proposal(by, built);
        signed
    }
}

type DealProposalOf<T> =
    DealProposal<<T as frame_system::Config>::AccountId, BalanceOf<T>, BlockNumberFor<T>>;

type ClientDealProposalOf<T> = ClientDealProposal<
    <T as frame_system::Config>::AccountId,
    BalanceOf<T>,
    BlockNumberFor<T>,
    MultiSignature,
>;

fn sign(pair: &sp_core::sr25519::Pair, bytes: &[u8]) -> MultiSignature {
    MultiSignature::Sr25519(pair.sign(bytes))
}

fn sign_proposal(client: &str, proposal: DealProposalOf<Test>) -> ClientDealProposalOf<Test> {
    let alice_pair = key_pair(client);
    let client_signature = sign(&alice_pair, &Encode::encode(&proposal));
    ClientDealProposal {
        proposal,
        client_signature,
    }
}
