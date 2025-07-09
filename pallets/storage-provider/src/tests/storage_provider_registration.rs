use frame_support::{assert_noop, assert_ok};
use frame_system::pallet_prelude::BlockNumberFor;
use primitives::proofs::RegisteredPoStProof;
use sp_core::{bounded_vec, Get};
use sp_runtime::{BoundedVec, DispatchError};

use super::{new_test_ext, Balances};
use crate::{
    pallet::{Error, Event, StorageProviders},
    sector::{TerminateSectorsParams, TerminationDeclaration},
    storage_provider::StorageProviderInfo,
    tests::{
        account, declare_faults::setup_sp_with_one_sector, events, publish_deals,
        register_storage_provider, run_to_block, sector_set, DealParametersBuilder,
        DealProposalBuilder, RuntimeEvent, RuntimeOrigin, SectorPreCommitInfoBuilder,
        StorageProvider, System, Test, ALICE, BOB, CHARLIE,
    },
    Config, SPDealParameters,
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
        let offchain_deal_params = DealParametersBuilder::default().build();
        let deal_params = offchain_deal_params
            .clone()
            .validate(
                <<Test as Config>::MinDealDuration as Get<BlockNumberFor<Test>>>::get(),
                <<Test as Config>::MaxDealDuration as Get<BlockNumberFor<Test>>>::get(),
            )
            .expect("Seamless conversion");

        // Register BOB as a storage provider.
        assert_ok!(StorageProvider::register_storage_provider(
            RuntimeOrigin::signed(account(BOB)),
            peer_id.clone(),
            window_post_type,
            offchain_deal_params.clone(),
        ));
        assert!(StorageProviders::<Test>::contains_key(account(BOB)));
        assert!(SPDealParameters::<Test>::contains_key(account(BOB)));

        let bob_deal_params = SPDealParameters::<Test>::get(account(BOB)).unwrap();
        assert_eq!(bob_deal_params, deal_params);
        // `unwrap()` should be safe because of the above check.
        let sp_bob = StorageProviders::<Test>::get(account(BOB)).unwrap();
        // Check that storage provider information is correct.
        assert_eq!(sp_bob.info.multiaddr, peer_id);
        assert_eq!(sp_bob.info.window_post_proof_type, window_post_type);
        assert_eq!(sp_bob.info.sector_size, expected_sector_size);
        assert_eq!(
            sp_bob.info.window_post_partition_sectors,
            expected_partition_sectors
        );
        // Check that pre commit sectors are empty.
        assert!(sp_bob.pre_committed_sectors.is_empty());
        // Check that no pre commit deposit is made
        assert_eq!(Balances::reserved_balance(&account(BOB)), 0);
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
                    deal_parameters: deal_params,
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
        // Default price = 5, default duration = 10
        let offchain_deal_params = DealParametersBuilder::default().build();

        assert_noop!(
            StorageProvider::register_storage_provider(
                RuntimeOrigin::none(),
                peer_id.clone(),
                window_post_type,
                offchain_deal_params
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
        // Default price = 5, default duration = 10
        let offchain_deal_params = DealParametersBuilder::default().build();

        // Register BOB as a storage provider.
        assert_ok!(StorageProvider::register_storage_provider(
            RuntimeOrigin::signed(account(BOB)),
            peer_id.clone(),
            window_post_type,
            offchain_deal_params.clone()
        ));
        assert!(StorageProviders::<Test>::contains_key(account(BOB)));
        // Try to register BOB again. Should fail
        assert_noop!(
            StorageProvider::register_storage_provider(
                RuntimeOrigin::signed(account(BOB)),
                peer_id.clone(),
                window_post_type,
                offchain_deal_params
            ),
            Error::<Test>::StorageProviderExists
        );
    });
}

#[test]
fn successful_deregistration_no_deals() {
    new_test_ext().execute_with(|| {
        // Register storage provider
        register_storage_provider(account(ALICE));
        let sp = StorageProviders::<Test>::get(account(ALICE)).unwrap();

        // Attempt to de-register ALICE
        assert_ok!(StorageProvider::deregister_storage_provider(
            RuntimeOrigin::signed(account(ALICE))
        ));

        assert!(!StorageProviders::<Test>::contains_key(account(ALICE)));
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(
                Event::<Test>::StorageProviderDeregistered {
                    owner: account(ALICE),
                    info: sp.info
                }
            )]
        );
    })
}

#[test]
fn successful_deregistration_after_sector_termination() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;
        setup_sp_with_one_sector(storage_provider, storage_client);
        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();

        // Terminate sectors so we can deregister
        let deadline = 0;
        let partition_num = 0;
        let sector = 0;
        let params = TerminateSectorsParams {
            terminations: bounded_vec![TerminationDeclaration {
                deadline,
                partition: partition_num,
                sectors: sector_set(&[sector])
            }],
        };

        assert_ok!(StorageProvider::terminate_sectors(
            RuntimeOrigin::signed(account(storage_provider)),
            params
        ));
        System::reset_events();

        assert_ok!(StorageProvider::deregister_storage_provider(
            RuntimeOrigin::signed(account(storage_provider))
        ));
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(
                Event::<Test>::StorageProviderDeregistered {
                    owner: account(storage_provider),
                    info: sp.info
                }
            )]
        );
    });
}

#[test]
fn successful_deregistration_after_sector_expiration() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;
        setup_sp_with_one_sector(storage_provider, storage_client);
        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();

        // setup_sp_with_one_sector uses `DealProposalBuilder` default expiration.
        // using the value from there in case the default changes
        let expiration_block = DealProposalBuilder::default().end_block;
        // expiration_block + FaultMaxAge = sector terminated by the system.
        run_to_block(expiration_block + <Test as Config>::FaultMaxAge::get());

        System::reset_events();

        assert_ok!(StorageProvider::deregister_storage_provider(
            RuntimeOrigin::signed(account(storage_provider))
        ));

        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(
                Event::<Test>::StorageProviderDeregistered {
                    owner: account(storage_provider),
                    info: sp.info
                }
            )]
        );
    })
}

#[test]
fn fails_deregistration_with_active_deals() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;
        setup_sp_with_one_sector(storage_provider, storage_client);

        assert_noop!(
            StorageProvider::deregister_storage_provider(RuntimeOrigin::signed(account(
                storage_provider
            ))),
            Error::<Test>::SPHasActiveDeals
        );

        assert_eq!(events(), []);
    })
}

#[test]
fn fails_deregistration_not_registered() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            StorageProvider::deregister_storage_provider(RuntimeOrigin::signed(account(ALICE))),
            Error::<Test>::StorageProviderNotRegistered
        );
        assert_eq!(events(), []);
    });
}

#[test]
fn fails_deregistration_pre_committed_sectors() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = CHARLIE;

        // Register storage provider
        register_storage_provider(account(storage_provider));
        // Set-up dependencies in Market Pallet
        publish_deals(storage_provider);

        // Sector to be pre-committed and proven
        let sector_number = 1.into();

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

        assert_noop!(
            StorageProvider::deregister_storage_provider(RuntimeOrigin::signed(account(
                storage_provider
            ))),
            Error::<Test>::SPHasPreCommittedSectors
        );
        assert_eq!(events(), []);
    });
}
