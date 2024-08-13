use frame_support::{assert_ok, pallet_prelude::*, BoundedBTreeSet};
use primitives_proofs::SectorNumber;
use primitives_proofs::MAX_TERMINATIONS_PER_CALL;
use sp_core::bounded_vec;
use sp_runtime::BoundedVec;

use crate::{
    fault::{
        DeclareFaultsParams, DeclareFaultsRecoveredParams, FaultDeclaration, RecoveryDeclaration,
    },
    pallet::{Event, StorageProviders, DECLARATIONS_MAX},
    sector::ProveCommitSector,
    tests::{
        account, count_sector_faults_and_recoveries, declare_faults::default_fault_setup, events,
        new_test_ext, register_storage_provider, DealProposalBuilder, DeclareFaultsBuilder,
        DeclareFaultsRecoveredBuilder, Market, RuntimeEvent, RuntimeOrigin,
        SectorPreCommitInfoBuilder, StorageProvider, System, Test, ALICE, BOB,
    },
};

#[test]
fn declare_single_fault_recovered() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;

        default_fault_setup(storage_provider, storage_client);
        let deadline = 0;
        let partition = 0;
        let sectors = vec![1];

        // Fault declaration setup
        assert_ok!(StorageProvider::declare_faults(
            RuntimeOrigin::signed(account(storage_provider)),
            DeclareFaultsBuilder::default()
                .fault(deadline, partition, sectors.clone())
                .build(),
        ));

        // Flush events
        System::reset_events();

        // setup recovery and run extrinsic
        assert_ok!(StorageProvider::declare_faults_recovered(
            RuntimeOrigin::signed(account(storage_provider)),
            DeclareFaultsRecoveredBuilder::default()
                .fault_recovery(deadline, partition, sectors)
                .build(),
        ));

        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();

        let (faults, recoveries) = count_sector_faults_and_recoveries(&sp.deadlines);

        // 1 recovery and 0 faults.
        assert_eq!(recoveries, 1);
        assert_eq!(faults, 1);
        assert!(matches!(
            events()[..],
            [RuntimeEvent::StorageProvider(Event::FaultsRecovered { .. })]
        ));
    });
}

/// TODO(aidan46, #183, 2024-08-07): Create setup for multiple partitions
#[test]
#[ignore = "This requires adding multiple partitions by adding more sectors than MAX_SECTORS."]
fn multiple_partition_faults_recovered() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;

        let mut sectors = BoundedBTreeSet::new();
        sectors.try_insert(1).expect(&format!("Inserting a single element into a BoundedBTreeSet with a capacity of {MAX_TERMINATIONS_PER_CALL} should be infallible"));

        // Fault declaration setup, not relevant to this test that why it has its own scope
        {
            default_fault_setup(storage_provider, storage_client);

            let mut faults: Vec<FaultDeclaration> = vec![];
            // declare faults in 5 partitions
            for i in 1..6 {
                let fault = FaultDeclaration {
                    deadline: 0,
                    partition: i,
                    sectors: sectors.clone(),
                };
                faults.push(fault);
            }
            let faults: BoundedVec<FaultDeclaration, ConstU32<DECLARATIONS_MAX>> = faults.try_into().expect(&format!("Converting a Vec with length 5 into a BoundedVec with a capacity of {MAX_TERMINATIONS_PER_CALL} should be infallible"));

            assert_ok!(StorageProvider::declare_faults(
                RuntimeOrigin::signed(account(storage_provider)),
                DeclareFaultsParams {
                    faults: faults.clone()
                },
            ));

            // flush events
            System::reset_events();
        }

        // setup recovery
        let mut recoveries: Vec<RecoveryDeclaration> = vec![];
        for i in 0..5 {
            let recovery = RecoveryDeclaration {
                deadline: 0,
                partition: i,
                sectors: sectors.clone(),
            };
            recoveries.push(recovery);
        }
        let recoveries: BoundedVec<RecoveryDeclaration, ConstU32<DECLARATIONS_MAX>> = recoveries.try_into().expect(&format!("Converting a Vec with length 5 into a BoundedVec with a capacity of {MAX_TERMINATIONS_PER_CALL} should be infallible"));

        // run extrinsic
        assert_ok!(StorageProvider::declare_faults_recovered(
            RuntimeOrigin::signed(account(storage_provider)),
            DeclareFaultsRecoveredParams {
                recoveries: recoveries.clone(),
            }
        ));

        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();

        let (_, recovered) = count_sector_faults_and_recoveries(&sp.deadlines);

        assert_eq!(recovered, 5);
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(Event::FaultsRecovered {
                owner: account(storage_provider),
                recoveries
            })]
        );
    });
}

#[test]
fn multiple_deadline_faults_recovered() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;

        let partition = 0;
        let deadlines = vec![0, 1, 2, 3, 4];
        let sectors = vec![1];

        default_fault_setup(storage_provider, storage_client);

        // Fault declaration setup
        assert_ok!(StorageProvider::declare_faults(
            RuntimeOrigin::signed(account(storage_provider)),
            DeclareFaultsBuilder::default()
                .multiple_deadlines(deadlines.clone(), partition, sectors.clone())
                .build(),
        ));

        // Flush events
        System::reset_events();

        // setup recovery and run extrinsic
        assert_ok!(StorageProvider::declare_faults_recovered(
            RuntimeOrigin::signed(account(storage_provider)),
            DeclareFaultsRecoveredBuilder::default()
                .multiple_deadlines_recovery(deadlines, partition, sectors)
                .build(),
        ));

        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();

        let (faults, recovered) = count_sector_faults_and_recoveries(&sp.deadlines);

        // Check that all faults are recovered.
        assert_eq!(recovered, 5);
        assert_eq!(faults, 5);
        assert!(matches!(
            events()[..],
            [RuntimeEvent::StorageProvider(Event::FaultsRecovered { .. })]
        ));
    });
}

#[test]
fn multiple_sector_faults_recovered() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;

        let sectors = vec![0, 1, 2, 3, 4];

        // Fault declaration setup
        multi_sectors_setup(storage_provider, storage_client, &sectors);

        // setup recovery and run extrinsic
        assert_ok!(StorageProvider::declare_faults_recovered(
            RuntimeOrigin::signed(account(storage_provider)),
            DeclareFaultsRecoveredBuilder::default()
                .fault_recovery(0, 0, sectors)
                .build(),
        ));

        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();

        let (faults, recoveries) = count_sector_faults_and_recoveries(&sp.deadlines);
        // Check that all faults are recovered.
        assert_eq!(recoveries, 5);
        assert_eq!(faults, 5);
        assert!(matches!(
            events()[..],
            [RuntimeEvent::StorageProvider(Event::FaultsRecovered { .. })]
        ));
    });
}

fn multi_sectors_setup(storage_provider: &str, storage_client: &str, sectors: &[SectorNumber]) {
    // Register storage provider
    register_storage_provider(account(storage_provider));

    // Add balance to the market pallet
    assert_ok!(Market::add_balance(
        RuntimeOrigin::signed(account(storage_provider)),
        250
    ));
    assert_ok!(Market::add_balance(
        RuntimeOrigin::signed(account(storage_client)),
        250
    ));
    for sector_number in 0..5 {
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
        let sector = SectorPreCommitInfoBuilder::default()
            .sector_number(sector_number)
            .deals(bounded_vec![sector_number])
            .build();

        // Run pre commit extrinsic
        assert_ok!(StorageProvider::pre_commit_sector(
            RuntimeOrigin::signed(account(storage_provider)),
            sector.clone()
        ));

        // Prove commit sector
        let sector = ProveCommitSector {
            sector_number,
            proof: bounded_vec![0xd, 0xe, 0xa, 0xd],
        };

        assert_ok!(StorageProvider::prove_commit_sector(
            RuntimeOrigin::signed(account(storage_provider)),
            sector
        ));
    }

    // Run extrinsic
    assert_ok!(StorageProvider::declare_faults(
        RuntimeOrigin::signed(account(storage_provider)),
        DeclareFaultsBuilder::default()
            .fault(0, 0, sectors.into())
            .build(),
    ));

    // Flush events
    System::reset_events();
}
