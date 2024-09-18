use frame_support::{assert_err, assert_noop, assert_ok};
use rstest::rstest;
use sp_core::{bounded_vec, ConstU32};
use sp_runtime::{traits::BlockNumberProvider, BoundedVec};

use crate::{
    deadline::{DeadlineError, DeadlineInfo, Deadlines},
    fault::{DeclareFaultsRecoveredParams, RecoveryDeclaration},
    pallet::{Error, Event, StorageProviders, DECLARATIONS_MAX},
    tests::{
        account, count_sector_faults_and_recoveries, create_set,
        declare_faults::{
            assert_exact_faulty_sectors, setup_sp_with_many_sectors_multiple_partitions,
            setup_sp_with_one_sector,
        },
        events, new_test_ext, run_to_block, DeclareFaultsBuilder, DeclareFaultsRecoveredBuilder,
        RuntimeEvent, RuntimeOrigin, StorageProvider, SubmitWindowedPoStBuilder, System, Test,
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
                .fault(deadline, partition, &sectors)
                .build(),
        ));

        // Flush events
        System::reset_events();

        // setup recovery and run extrinsic
        assert_ok!(StorageProvider::declare_faults_recovered(
            RuntimeOrigin::signed(account(storage_provider)),
            DeclareFaultsRecoveredBuilder::default()
                .fault_recovery(deadline, partition, &sectors)
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
                .fault(deadline, partition, &sectors)
                .build(),
        ));
        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();
        let (faults, recoveries) = count_sector_faults_and_recoveries(&sp.deadlines);
        // 0 recovery and 1 faults.
        assert_eq!(recoveries, 0);
        assert_eq!(faults, 1);

        run_to_block(sp.proving_period_start + 1);

        // when the deadline started, it shouldn't be possible to recover it.
        assert_err!(
            StorageProvider::declare_faults_recovered(
                RuntimeOrigin::signed(account(storage_provider)),
                DeclareFaultsRecoveredBuilder::default()
                    .fault_recovery(deadline, partition, &sectors)
                    .build(),
            ),
            Error::<Test>::FaultRecoveryTooLate
        );

        let proving_period = <Test as Config>::WPoStProvingPeriod::get();
        let fault_declaration_cuttoff = <Test as Config>::FaultDeclarationCutoff::get();

        // before the next deadline happens and before the cutoff!
        run_to_block(sp.proving_period_start + proving_period - fault_declaration_cuttoff - 1);
        assert_ok!(StorageProvider::declare_faults_recovered(
            RuntimeOrigin::signed(account(storage_provider)),
            DeclareFaultsRecoveredBuilder::default()
                .fault_recovery(deadline, partition, &sectors)
                .build(),
        ));
        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();
        let (faults, recoveries) = count_sector_faults_and_recoveries(&sp.deadlines);
        // 1 recovery and 1 faults.
        assert_eq!(recoveries, 1);
        assert_eq!(faults, 1);

        // the next deadline time!
        run_to_block(sp.proving_period_start + proving_period + 1);
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

#[test]
fn successfully_recover_multiple_sector_faults() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;

        // Sectors setup
        setup_sp_with_many_sectors_multiple_partitions(storage_provider, storage_client);

        // We should specify a correct partition and deadline for the sector
        // when specifying the faults
        let fault_declaration = DeclareFaultsBuilder::default()
            .fault(0, 0, &[0, 1])
            .fault(0, 1, &[20, 21])
            .fault(1, 0, &[2, 3])
            .fault(2, 0, &[4])
            .build();
        assert_ok!(StorageProvider::declare_faults(
            RuntimeOrigin::signed(account(storage_provider)),
            fault_declaration.clone(),
        ));

        // Flush events
        System::reset_events();

        // We should specify a correct partition and deadline for the sector
        // when specifying the faults recovered
        let recovery_declaration = DeclareFaultsRecoveredBuilder::default()
            .fault_recovery(0, 0, &[0, 1])
            .fault_recovery(0, 1, &[20, 21])
            .fault_recovery(1, 0, &[2, 3])
            .fault_recovery(2, 0, &[4])
            .build();
        assert_ok!(StorageProvider::declare_faults_recovered(
            RuntimeOrigin::signed(account(storage_provider)),
            recovery_declaration.clone()
        ));

        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();
        assert_exact_faulty_sectors(&sp.deadlines, &fault_declaration.faults);
        assert_exact_recovered_sectors(&sp.deadlines, &recovery_declaration.recoveries);
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
            <Test as Config>::WPoStPeriodDeadlines::get(),
            <Test as Config>::WPoStProvingPeriod::get(),
            <Test as Config>::WPoStChallengeWindow::get(),
            <Test as Config>::WPoStChallengeLookBack::get(),
            <Test as Config>::FaultDeclarationCutoff::get(),
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
                    .fault_recovery(deadline, partition, &[1])
                    .build(),
            ),
            Error::<Test>::FaultRecoveryTooLate
        );
    });
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
