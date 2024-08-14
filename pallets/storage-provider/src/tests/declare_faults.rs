use frame_support::{assert_noop, assert_ok};
use rstest::rstest;
use sp_core::{bounded_vec, ConstU32};
use sp_runtime::{BoundedVec, DispatchError};

use crate::{
    deadline::{DeadlineError, Deadlines},
    fault::{DeclareFaultsParams, FaultDeclaration},
    pallet::{Error, Event, StorageProviders, DECLARATIONS_MAX},
    partition::PartitionError,
    sector::ProveCommitSector,
    sector_map::SectorMapError,
    tests::{
        account, create_set, events, new_test_ext, register_storage_provider, DealProposalBuilder,
        DeclareFaultsBuilder, Market, RuntimeEvent, RuntimeOrigin, SectorPreCommitInfoBuilder,
        StorageProvider, System, Test, ALICE, BOB, CHARLIE,
    },
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
fn declare_single_fault() {
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
            .fault(deadline, partition, sectors)
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
        setup_sp_with_one_sector(storage_provider, storage_client);

        let partition = 0;
        let deadlines = vec![0, 1, 2, 3, 4];
        let sectors = vec![1];

        // Fault declaration and extrinsic
        let fault_declaration = DeclareFaultsBuilder::default()
            .multiple_deadlines(deadlines, partition, sectors)
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
], Error::<Test>::SectorMapError(SectorMapError::EmptySectors).into())]
// Deadline specified is not valid
#[case(bounded_vec![
    FaultDeclaration {
        deadline: 99,
        partition: 0,
        sectors: create_set(&[0]),
    },
], Error::<Test>::DeadlineError(DeadlineError::DeadlineIndexOutOfRange).into())]
// Partition specified is not used
#[case(bounded_vec![
    FaultDeclaration {
        deadline: 0,
        partition: 99,
        sectors: create_set(&[0]),
    },
], Error::<Test>::DeadlineError(DeadlineError::PartitionNotFound).into())]
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

/// Setup storage provider with one sector.
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

    // Flush events before running extrinsic to check only relevant events
    System::reset_events();
}

/// Setup storage provider with many sectors and multiple partitions.
///
/// The storage provider has 10 deadlines with 2 partitions each. The first
/// deadline has 3 partitions. The third partition is partially filled.
///
/// Deadlines:
/// - Deadline 0:
///     Partition 0:
///         Sectors 0, 1
///     Partition 1:
///         Sectors 20, 21
///     Partition 2:
///        Sectors 40
/// - Deadline 1:
///     Partition 0:
///         Sectors 2, 3
///     Partition 1:
///         Sectors 22, 23
///
/// ....................
///
/// - Deadline 10:
///     Partition 0:
///         Sectors 18, 19
///     Partition 1:
///         Sectors 38, 39
pub(crate) fn setup_sp_with_many_sectors_multiple_partitions(
    storage_provider: &str,
    storage_client: &str,
) {
    // Register storage provider
    register_storage_provider(account(storage_provider));

    // We are making so that each deadline have at least two partitions. The
    // first deadline has three with third sector only partially filled.
    let desired_sectors = 10 * (2 + 2) + 1; // 10 deadlines with 2 partitions each partition have 2 sectors

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
        assert_ok!(StorageProvider::pre_commit_sector(
            RuntimeOrigin::signed(account(storage_provider)),
            sector.clone()
        ));

        // Prove commit sector
        let sector = ProveCommitSector {
            sector_number,
            proof: bounded_vec![0xb, 0xe, 0xe, 0xf],
        };

        assert_ok!(StorageProvider::prove_commit_sector(
            RuntimeOrigin::signed(account(storage_provider)),
            sector
        ));
    }

    // Flush events before running extrinsic to check only relevant events
    System::reset_events();
}

/// Compare faults in deadlines and faults expected. Panic if faults in both are
/// not equal.
fn assert_exact_faulty_sectors(
    deadlines: &Deadlines<u64>,
    expected_faults: &BoundedVec<FaultDeclaration, ConstU32<DECLARATIONS_MAX>>,
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
