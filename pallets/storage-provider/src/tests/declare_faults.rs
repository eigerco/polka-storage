use frame_support::{assert_err, assert_noop, assert_ok, pallet_prelude::*};
use rstest::rstest;
use sp_core::bounded_vec;
use sp_runtime::{traits::BlockNumberProvider, BoundedVec};

use crate::{
    deadline::{DeadlineInfo, Deadlines},
    error::GeneralPalletError,
    fault::{DeclareFaultsParams, FaultDeclaration},
    pallet::{Error, Event, StorageProviders, DECLARATIONS_MAX},
    sector::ProveCommitSector,
    tests::{
        account, create_set, events, new_test_ext, register_storage_provider, run_to_block,
        DealProposalBuilder, DeclareFaultsBuilder, Market, RuntimeEvent, RuntimeOrigin,
        SectorPreCommitInfoBuilder, StorageProvider, System, Test, ALICE, BOB, CHARLIE,
    },
    Config,
};

#[test]
fn fails_should_be_signed() {
    new_test_ext().execute_with(|| {
        // Build faults
        let faults: BoundedVec<_, _> = bounded_vec![FaultDeclaration {
            deadline: 0,
            partition: 0,
            sectors: create_set(&[1, 2, 3, 4, 5]),
        }];

        assert_noop!(
            StorageProvider::declare_faults(RuntimeOrigin::none(), DeclareFaultsParams { faults },),
            DispatchError::BadOrigin
        );
    });
}

#[test]
fn multiple_sector_faults() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;

        // Setup
        setup_sp_with_many_sectors_multiple_partitions(storage_provider, storage_client);

        // Flush events before running extrinsic to check only relevant events
        System::reset_events();

        let faults: BoundedVec<_, _> = bounded_vec![FaultDeclaration {
            deadline: 0,
            partition: 0,
            sectors: create_set(&[0, 1]),
        }];

        assert_ok!(StorageProvider::declare_faults(
            RuntimeOrigin::signed(account(storage_provider)),
            DeclareFaultsParams {
                faults: faults.clone()
            },
        ));

        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();
        assert_exact_faulty_sectors(&sp.deadlines, &faults);
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(Event::FaultsDeclared {
                owner: account(storage_provider),
                faults
            })]
        );
    });
}

#[test]
fn declare_single_fault_before_proving_period_start() {
    new_test_ext().execute_with(|| {
        // Setup
        let storage_provider = ALICE;
        let storage_client = BOB;
        setup_sp_with_one_sector(storage_provider, storage_client);

        let deadline = 0;
        let partition = 0;
        let sectors = vec![0];

        // Fault declaration setup
        let fault_declaration = DeclareFaultsBuilder::default()
            .fault(deadline, partition, &sectors)
            .build();
        assert_ok!(StorageProvider::declare_faults(
            RuntimeOrigin::signed(account(storage_provider)),
            fault_declaration.clone(),
        ));

        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();
        assert_exact_faulty_sectors(&sp.deadlines, &fault_declaration.faults);
        assert!(matches!(
            events()[..],
            [RuntimeEvent::StorageProvider(Event::FaultsDeclared { .. })]
        ));

        // Check the expiration blocks
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

        // duplicated loop (because of the count_sectors_...) but that's ok
        for dl in sp.deadlines.due.iter() {
            for (partition_number, _) in dl
                .partitions
                .iter()
                .filter(|(_, partition)| partition.faults.len() == 1)
            {
                assert_eq!(
                    dl.expirations_blocks
                        .get(&(test_dl.last() + <Test as Config>::FaultMaxAge::get()))
                        .expect("should exist"),
                    partition_number
                );
            }
        }
    });
}

// Using floats is the easiest way to specify a multiple of the proving period
#[rstest]
#[case(0.1)]
#[case(0.5)]
#[case(1.0)]
fn declare_single_fault_from_proving_period(#[case] proving_period_multiple: f64) {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;
        setup_sp_with_one_sector(storage_provider, storage_client);

        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();
        // Generally safe because we control test conditions, not really safe anywhere else
        let offset = ((sp.proving_period_start as f64) * proving_period_multiple) as u64;
        let new_block = sp.proving_period_start + offset;
        run_to_block(new_block);
        // The cron hook generates events between blocks, this removes those events
        System::reset_events();

        let sectors = create_set(&[0]);
        let fault = FaultDeclaration {
            deadline: 0,
            partition: 0,
            sectors: sectors.clone(),
        };
        assert_ok!(StorageProvider::declare_faults(
            RuntimeOrigin::signed(account(storage_provider)),
            DeclareFaultsParams {
                faults: bounded_vec![fault.clone()]
            },
        ));

        // Second call because we've updated the state and StorageProviders returns a clone of the state
        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();
        assert_exact_faulty_sectors(&sp.deadlines, &[fault.clone()]);
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(Event::FaultsDeclared {
                owner: account(storage_provider),
                faults: bounded_vec![fault]
            })]
        );

        let test_dl = DeadlineInfo::new(
            new_block,
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

        for dl in sp.deadlines.due.iter() {
            for (partition_number, _) in dl
                .partitions
                .iter()
                .filter(|(_, partition)| partition.faults.len() == 1)
            {
                assert_eq!(
                    dl.expirations_blocks
                        .get(&(test_dl.last() + <Test as Config>::FaultMaxAge::get()))
                        .expect("should exist"),
                    partition_number
                );
            }
        }
    });
}

#[test]
fn multiple_partition_faults_in_same_deadline() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = CHARLIE;
        let storage_client = ALICE;
        setup_sp_with_many_sectors_multiple_partitions(storage_provider, storage_client);

        let faults: BoundedVec<_, _> = bounded_vec![
            FaultDeclaration {
                deadline: 0,
                partition: 0,
                sectors: create_set(&[0, 1]),
            },
            FaultDeclaration {
                deadline: 0,
                partition: 1,
                sectors: create_set(&[20]),
            },
        ];

        assert_ok!(StorageProvider::declare_faults(
            RuntimeOrigin::signed(account(storage_provider)),
            DeclareFaultsParams {
                faults: faults.clone()
            },
        ));

        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();
        assert_exact_faulty_sectors(&sp.deadlines, &faults);
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(Event::FaultsDeclared {
                owner: account(storage_provider),
                faults
            })]
        );
    });
}

#[test]
fn multiple_deadline_faults() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;
        setup_sp_with_many_sectors_multiple_partitions(storage_provider, storage_client);

        // We should specify a correct partition and deadline for the sector
        // when specifying the faults
        let fault_declaration = DeclareFaultsBuilder::default()
            .fault(0, 0, &[0])
            .fault(1, 0, &[2])
            .fault(2, 0, &[4])
            .fault(3, 0, &[6])
            .fault(4, 0, &[8])
            .build();
        assert_ok!(StorageProvider::declare_faults(
            RuntimeOrigin::signed(account(storage_provider)),
            fault_declaration.clone(),
        ));

        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();
        assert_exact_faulty_sectors(&sp.deadlines, &fault_declaration.faults);
        assert!(matches!(
            events()[..],
            [RuntimeEvent::StorageProvider(Event::FaultsDeclared { .. })]
        ));
    });
}

#[rstest]
// No sectors declared as faulty
#[case(bounded_vec![
    FaultDeclaration {
        deadline: 0,
        partition: 0,
        sectors: create_set(&[]),
    },
], Error::<Test>::GeneralPalletError(GeneralPalletError::DeadlineErrorCouldNotAddSectors).into())]
// Deadline specified is not valid
#[case(bounded_vec![
    FaultDeclaration {
        deadline: 99,
        partition: 0,
        sectors: create_set(&[0]),
    },
], Error::<Test>::GeneralPalletError(GeneralPalletError::DeadlineErrorDeadlineIndexOutOfRange).into())]
// Partition specified is not used
#[case(bounded_vec![
    FaultDeclaration {
        deadline: 0,
        partition: 99,
        sectors: create_set(&[0]),
    },
], Error::<Test>::GeneralPalletError(GeneralPalletError::DeadlineErrorPartitionNotFound).into())]
#[case(bounded_vec![
    FaultDeclaration {
        deadline: 0,
        partition: 0,
        sectors: create_set(&[99]),
     },
], Error::<Test>::GeneralPalletError(GeneralPalletError::DeadlineErrorSectorsNotFound).into())]
fn fails_data_missing_malformed(
    #[case] declared_faults: BoundedVec<FaultDeclaration, ConstU32<DECLARATIONS_MAX>>,
    #[case] expected_error: Error<Test>,
) {
    new_test_ext().execute_with(|| {
        // Setup storage provider data
        let storage_provider = CHARLIE;
        let storage_client = ALICE;
        setup_sp_with_one_sector(storage_provider, storage_client);

        // Declare faults
        assert_noop!(
            StorageProvider::declare_faults(
                RuntimeOrigin::signed(account(storage_provider)),
                DeclareFaultsParams {
                    faults: declared_faults
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
fn fault_declaration_past_cutoff_should_fail() {
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
            StorageProvider::declare_faults(
                RuntimeOrigin::signed(account(storage_provider)),
                DeclareFaultsBuilder::default()
                    .fault(deadline, partition, &[1])
                    .build(),
            ),
            Error::<Test>::FaultDeclarationTooLate
        );
    });
}

pub(crate) fn setup_sp_with_one_sector(storage_provider: &str, storage_client: &str) {
    // Register storage provider
    register_storage_provider(account(storage_provider));

    // Add balance to the market pallet
    assert_ok!(Market::add_balance(
        RuntimeOrigin::signed(account(storage_provider)),
        60
    ));
    assert_ok!(Market::add_balance(
        RuntimeOrigin::signed(account(storage_client)),
        70
    ));

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
    let sector_number = 0;

    // Sector data
    let sector = SectorPreCommitInfoBuilder::default()
        .sector_number(sector_number)
        .deals(vec![0])
        .build();

    // Run pre commit extrinsic
    assert_ok!(StorageProvider::pre_commit_sectors(
        RuntimeOrigin::signed(account(storage_provider)),
        bounded_vec![sector.clone()]
    ));

    // Prove commit sector
    let sector = ProveCommitSector {
        sector_number,
        proof: bounded_vec![0xd, 0xe, 0xa, 0xd],
    };

    assert_ok!(StorageProvider::prove_commit_sectors(
        RuntimeOrigin::signed(account(storage_provider)),
        bounded_vec![sector]
    ));

    // Flush events before running extrinsic to check only relevant events
    System::reset_events();
}

/// Setup storage provider with many sectors and multiple partitions.
///
/// The storage provider has 10 deadlines with at least 2 partitions in each
/// deadline. The first deadline has 3 partitions. The third partition is
/// partially filled.
///
/// Deadlines:
/// - Deadline 0:
///     - Partition 0:
///         - Sectors 0, 1
///     - Partition 1:
///         - Sectors 20, 21
///     - Partition 2:
///        - Sectors 40
/// - Deadline 1:
///     - Partition 0:
///         - Sectors 2, 3
///     - Partition 1:
///         - Sectors 22, 23
///
/// ....................
///
/// - Deadline 10:
///     - Partition 0:
///         - Sectors 18, 19
///     - Partition 1:
///         - Sectors 38, 39
pub(crate) fn setup_sp_with_many_sectors_multiple_partitions(
    storage_provider: &str,
    storage_client: &str,
) {
    // Register storage provider
    register_storage_provider(account(storage_provider));

    // We are making so that each deadline have at least two partitions. The
    // first deadline has three with third sector only partially filled.
    //
    // 10 deadlines with 2 partitions each partition have 2 sectors
    let desired_sectors = 10 * (2 + 2) + 1;

    // Publish as many deals as we need to fill the sectors. We are batching
    // deals so that the processing is a little faster.
    let deal_ids = {
        // Amounts needed for deals
        let provider_amount_needed = desired_sectors * 70;
        let client_amount_needed = desired_sectors * 60;

        // Move available balance of provider to the market pallet
        assert_ok!(Market::add_balance(
            RuntimeOrigin::signed(account(storage_provider)),
            provider_amount_needed
        ));

        // Move available balance of client to the market pallet
        assert_ok!(Market::add_balance(
            RuntimeOrigin::signed(account(storage_client)),
            client_amount_needed
        ));

        // Deal proposals
        let deal_ids = 0..desired_sectors;
        let proposals = deal_ids
            .clone()
            .map(|deal_id| {
                // Generate a deal proposal
                DealProposalBuilder::default()
                    .client(storage_client)
                    .provider(storage_provider)
                    // We are setting a label here so that our deals are unique
                    .label(vec![deal_id as u8])
                    .signed(storage_client)
            })
            .collect::<Vec<_>>();

        // Publish all proposals
        assert_ok!(Market::publish_storage_deals(
            RuntimeOrigin::signed(account(storage_provider)),
            proposals.try_into().unwrap(),
        ));

        deal_ids
    };

    // Pre commit and prove commit sectors
    for id in deal_ids {
        // We are reusing deal_id as sector_number. In this case this is ok
        // because we wan't to have a unique sector for each deal. Usually
        // we would pack multiple deals in the same sector
        let sector_number = id;

        // Sector data
        let sector = SectorPreCommitInfoBuilder::default()
            .sector_number(sector_number)
            .deals(vec![id])
            .build();

        // Run pre commit extrinsic
        assert_ok!(StorageProvider::pre_commit_sectors(
            RuntimeOrigin::signed(account(storage_provider)),
            bounded_vec![sector.clone()]
        ));

        // Prove commit sector
        let sector = ProveCommitSector {
            sector_number,
            proof: bounded_vec![0xb, 0xe, 0xe, 0xf],
        };

        assert_ok!(StorageProvider::prove_commit_sectors(
            RuntimeOrigin::signed(account(storage_provider)),
            bounded_vec![sector]
        ));
    }

    // Flush events before running extrinsic to check only relevant events
    System::reset_events();
}

/// Compare faults in deadlines and faults expected. Panic if faults in both are
/// not equal.
pub(crate) fn assert_exact_faulty_sectors(
    deadlines: &Deadlines<u64>,
    expected_faults: &[FaultDeclaration],
) {
    // Faulty sectors specified in the faults
    let faults_sectors = expected_faults
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
                .flat_map(|(_, p)| p.faults.iter().collect::<Vec<_>>())
        })
        .collect::<Vec<_>>();

    // Should be equal
    assert_eq!(faults_sectors, deadline_sectors);
}
