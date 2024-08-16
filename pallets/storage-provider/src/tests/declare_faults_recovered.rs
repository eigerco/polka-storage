use frame_support::{assert_noop, assert_ok, assert_err,pallet_prelude::*, BoundedBTreeSet};
use primitives_proofs::{SectorNumber, MAX_TERMINATIONS_PER_CALL};
use rstest::rstest;
use sp_core::bounded_vec;
use sp_runtime::{traits::BlockNumberProvider, BoundedVec};

use crate::{
    deadline::{DeadlineError, DeadlineInfo, Deadlines},
    fault::{
        DeclareFaultsParams, DeclareFaultsRecoveredParams, FaultDeclaration, RecoveryDeclaration,
    },
    pallet::{Error, Event, StorageProviders, DECLARATIONS_MAX},
    sector::ProveCommitSector,
    tests::{
        account, count_sector_faults_and_recoveries, create_set,
        declare_faults::{
            assert_exact_faulty_sectors, setup_sp_with_many_sectors_multiple_partitions,
            setup_sp_with_one_sector,
        },
        events, new_test_ext, register_storage_provider, run_to_block, DealProposalBuilder,
        DeclareFaultsBuilder, DeclareFaultsRecoveredBuilder, Market, RuntimeEvent, RuntimeOrigin,
        SectorPreCommitInfoBuilder, StorageProvider, SubmitWindowedPoStBuilder, System, Test,
        ALICE, BOB,
    },
    Config,
};

#[test]
fn declare_single_fault_recovered() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;
        setup_sp_with_one_sector(storage_provider, storage_client);

        let deadline = 0;
        let partition = 0;
        let sectors = vec![0];

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

        // 1 recovery and 1 faults.
        assert_eq!(recoveries, 1);
        assert_eq!(faults, 1);
        assert!(matches!(
            events()[..],
            [RuntimeEvent::StorageProvider(Event::FaultsRecovered { .. })]
        ));
    });
}

#[test]
fn declare_single_fault_recovered_and_submitted() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;

        setup_sp_with_one_sector(storage_provider, storage_client);
        let deadline = 0;
        let partition = 0;
        let sectors = vec![0];

        // Fault declaration setup
        assert_ok!(StorageProvider::declare_faults(
            RuntimeOrigin::signed(account(storage_provider)),
            DeclareFaultsBuilder::default()
                .fault(deadline, partition, sectors.clone())
                .build(),
        ));
        assert_ok!(StorageProvider::declare_faults_recovered(
            RuntimeOrigin::signed(account(storage_provider)),
            DeclareFaultsRecoveredBuilder::default()
                .fault_recovery(deadline, partition, sectors)
                .build(),
        ));
        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();
        let (faults, recoveries) = count_sector_faults_and_recoveries(&sp.deadlines);
        // 1 recovery and 1 faults.
        assert_eq!(recoveries, 1);
        assert_eq!(faults, 1);
        run_to_block(sp.proving_period_start + 1);

        // Flush events
        System::reset_events();

        let windowed_post = SubmitWindowedPoStBuilder::default()
            .partition(partition)
            .build();
        assert_ok!(StorageProvider::submit_windowed_post(
            RuntimeOrigin::signed(account(ALICE)),
            windowed_post.clone(),
        ));

        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();
        let (faults, recoveries) = count_sector_faults_and_recoveries(&sp.deadlines);
        // 0 recovery and 0 faults.
        assert_eq!(recoveries, 0);
        assert_eq!(faults, 0);
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
            setup_sp_with_one_sector(storage_provider, storage_client);

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
        let sectors = vec![0, 1, 2, 3, 4];

        setup_sp_with_many_sectors_multiple_partitions(storage_provider, storage_client);

        let fault_declaration = DeclareFaultsBuilder::default()
            .multiple_deadlines(deadlines.clone(), partition, sectors.clone())
            .build();

        // Fault declaration setup
        assert_ok!(StorageProvider::declare_faults(
            RuntimeOrigin::signed(account(storage_provider)),
            fault_declaration.clone(),
        ));

        // Flush events
        System::reset_events();

        let recovery_declaration = DeclareFaultsRecoveredBuilder::default()
            .multiple_deadlines_recovery(deadlines, partition, sectors)
            .build();

        // setup recovery and run extrinsic
        assert_ok!(StorageProvider::declare_faults_recovered(
            RuntimeOrigin::signed(account(storage_provider)),
            recovery_declaration.clone()
        ));

        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();

        assert_exact_faulty_sectors(&sp.deadlines, &fault_declaration.faults);
        assert_exact_recovered_sectors(&sp.deadlines, &recovery_declaration.recoveries);
        // Check that all faults are recovered.
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
        multi_sectors_setup_fault_recovery(storage_provider, storage_client, &sectors);

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

#[rstest]
// No sectors declared as recovered
#[case(bounded_vec![
    RecoveryDeclaration {
        deadline: 0,
        partition: 0,
        sectors: create_set(&[]),
    },
], Error::<Test>::DeadlineError(DeadlineError::CouldNotAddSectors).into())]
// Deadline specified is not valid
#[case(bounded_vec![
    RecoveryDeclaration {
        deadline: 99,
        partition: 0,
        sectors: create_set(&[0]),
    },
], Error::<Test>::DeadlineError(DeadlineError::DeadlineIndexOutOfRange).into())]
// Partition specified is not used
#[case(bounded_vec![
    RecoveryDeclaration {
        deadline: 0,
        partition: 99,
        sectors: create_set(&[0]),
    },
], Error::<Test>::DeadlineError(DeadlineError::PartitionNotFound).into())]
#[case(bounded_vec![
    RecoveryDeclaration {
        deadline: 0,
        partition: 0,
        sectors: create_set(&[99]),
     },
], Error::<Test>::DeadlineError(DeadlineError::SectorsNotFound).into())]
#[case(bounded_vec![
    RecoveryDeclaration {
        deadline: 0,
        partition: 0,
        sectors: create_set(&[0]),
     },
], Error::<Test>::DeadlineError(DeadlineError::SectorsNotFaulty).into())]
fn fails_data_missing_malformed(
    #[case] declared_recoveries: BoundedVec<RecoveryDeclaration, ConstU32<DECLARATIONS_MAX>>,
    #[case] expected_error: Error<Test>,
) {
    new_test_ext().execute_with(|| {
        // Setup storage provider data
        let storage_provider = BOB;
        let storage_client = ALICE;
        setup_sp_with_one_sector(storage_provider, storage_client);

        // Declare faults
        assert_noop!(
            StorageProvider::declare_faults_recovered(
                RuntimeOrigin::signed(account(storage_provider)),
                DeclareFaultsRecoveredParams {
                    recoveries: declared_recoveries,
                },
            ),
            expected_error,
        );

        // Not sure if this is needed. Does the `assert_noop` above also checks
        // that no events were published?
        assert_eq!(events(), []);
    });
}

#[test]
fn fault_recovery_past_cutoff_should_fail() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;

        setup_sp_with_one_sector(storage_provider, storage_client);

        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();

        let test_dl = DeadlineInfo::new(
            System::current_block_number(),
            sp.proving_period_start,
            0,
            <Test as Config>::FaultDeclarationCutoff::get(),
            <Test as Config>::WPoStPeriodDeadlines::get(),
            <Test as Config>::WPoStChallengeWindow::get(),
            <Test as Config>::WPoStProvingPeriod::get(),
            <Test as Config>::WPoStChallengeLookBack::get(),
        )
        .and_then(DeadlineInfo::next_not_elapsed)
        .expect("deadline should be valid");

        // Run block to the fault declaration cutoff.
        run_to_block(test_dl.fault_cutoff);

        let deadline = 0;
        let partition = 0;
        // Fault declaration setup
        assert_err!(
            StorageProvider::declare_faults_recovered(
                RuntimeOrigin::signed(account(storage_provider)),
                DeclareFaultsRecoveredBuilder::default()
                    .fault_recovery(deadline, partition, vec![1])
                    .build(),
            ),
            Error::<Test>::FaultRecoveryTooLate
        );
    });
}

/// This function sets up 5 deals thus creating 5 sectors.
/// Similar to `multi_sectors_setup_fault_declaration` in the declare faults test but it runs the `declare_faults` extrinsic too.
/// SP Extrinsics run:
/// `pre_commit_sector`
/// `prove_commit_sector`
/// `declare_faults`
fn multi_sectors_setup_fault_recovery(
    storage_provider: &str,
    storage_client: &str,
    sectors: &[SectorNumber],
) {
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

/// Compare faults in deadlines and faults expected. Panic if faults in both are
/// not equal.
pub(crate) fn assert_exact_recovered_sectors(
    deadlines: &Deadlines<u64>,
    expected_recoveries: &[RecoveryDeclaration],
) {
    // Faulty sectors specified in the faults
    let recovered_sectors = expected_recoveries
        .iter()
        .flat_map(|f| f.sectors.iter().collect::<Vec<_>>())
        .collect::<Vec<_>>();

    // Faulted sectors in the deadlines
    let deadline_sectors = deadlines
        .due
        .iter()
        .flat_map(|dl| {
            dl.partitions
                .iter()
                .flat_map(|(_, p)| p.recoveries.iter().collect::<Vec<_>>())
        })
        .collect::<Vec<_>>();

    // Should be equal
    assert_eq!(recovered_sectors, deadline_sectors);
}
