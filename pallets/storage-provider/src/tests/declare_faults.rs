use frame_support::{assert_ok, pallet_prelude::*, BoundedBTreeSet};
use sp_core::bounded_vec;
use sp_runtime::BoundedVec;

use crate::{
    fault::{DeclareFaultsParams, FaultDeclaration},
    pallet::{Event, StorageProviders, DECLARATIONS_MAX},
    sector::ProveCommitSector,
    tests::{
        account, events, new_test_ext, register_storage_provider, DealProposalBuilder, Market,
        RuntimeEvent, RuntimeOrigin, SectorPreCommitInfoBuilder, StorageProvider, System, Test,
        ALICE, BOB,
    },
};

#[test]
fn multiple_sector_faults() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;

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
        for sector_number in 1..6 {
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
                .deals(vec![sector_number - 1])
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

        // Flush events before running extrinsic to check only relevant events
        System::reset_events();

        let mut sectors = BoundedBTreeSet::new();
        // insert 5 sectors
        for i in 1..6 {
            sectors.try_insert(i).expect("Programmer error");
        }
        let fault = FaultDeclaration {
            deadline: 0,
            partition: 0,
            sectors,
        };

        assert_ok!(StorageProvider::declare_faults(
            RuntimeOrigin::signed(account(storage_provider)),
            DeclareFaultsParams {
                faults: bounded_vec![fault.clone()]
            },
        ));

        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();

        let mut updates = 0;

        for dl in sp.deadlines.due.iter() {
            for (_, partition) in dl.partitions.iter() {
                if partition.faults.len() > 0 {
                    updates += partition.faults.len();
                }
            }
        }
        // One partitions fault should be added.
        assert_eq!(updates, 5);
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(Event::FaultsDeclared {
                owner: account(storage_provider),
                faults: bounded_vec![fault]
            })]
        );
    });
}

#[test]
fn declare_single_fault() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;

        default_fault_setup(storage_provider, storage_client);

        let mut sectors = BoundedBTreeSet::new();
        sectors.try_insert(1).expect("Programmer error");
        let fault = FaultDeclaration {
            deadline: 0,
            partition: 0,
            sectors,
        };
        assert_ok!(StorageProvider::declare_faults(
            RuntimeOrigin::signed(account(storage_provider)),
            DeclareFaultsParams {
                faults: bounded_vec![fault.clone()]
            },
        ));

        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();

        let mut updates = 0;

        for dl in sp.deadlines.due.iter() {
            for (_, partition) in dl.partitions.iter() {
                if partition.faults.len() > 0 {
                    updates += 1;
                }
            }
        }
        // One partitions fault should be added.
        assert_eq!(updates, 1);
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(Event::FaultsDeclared {
                owner: account(storage_provider),
                faults: bounded_vec![fault]
            })]
        );
    });
}

/// TODO(aidan46, #183, 2024-08-07): Create setup for multiple partitions
#[test]
#[ignore = "This requires adding multiple partitions by adding more sectors than MAX_SECTORS."]
fn multiple_partition_faults() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;

        default_fault_setup(storage_provider, storage_client);

        let mut sectors = BoundedBTreeSet::new();
        let mut faults: BoundedVec<FaultDeclaration, ConstU32<DECLARATIONS_MAX>> = bounded_vec![];
        sectors.try_insert(1).expect("Programmer error");
        // declare faults in 5 partitions
        for i in 1..6 {
            let fault = FaultDeclaration {
                deadline: 0,
                partition: i,
                sectors: sectors.clone(),
            };
            faults.try_push(fault).expect("Programmer error");
        }

        assert_ok!(StorageProvider::declare_faults(
            RuntimeOrigin::signed(account(storage_provider)),
            DeclareFaultsParams {
                faults: faults.clone()
            },
        ));

        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();

        let mut updates = 0;

        for dl in sp.deadlines.due.iter() {
            for (_, partition) in dl.partitions.iter() {
                if partition.faults.len() > 0 {
                    updates += partition.faults.len();
                }
            }
        }
        // One partitions fault should be added.
        assert_eq!(updates, 5);
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

        default_fault_setup(storage_provider, storage_client);

        let mut sectors = BoundedBTreeSet::new();
        sectors.try_insert(1).expect("Programmer error");
        let mut faults: BoundedVec<FaultDeclaration, ConstU32<DECLARATIONS_MAX>> = bounded_vec![];
        // declare faults in 5 partitions
        for i in 0..5 {
            let fault = FaultDeclaration {
                deadline: i,
                partition: 0,
                sectors: sectors.clone(),
            };
            faults.try_push(fault).expect("Programmer error");
        }

        assert_ok!(StorageProvider::declare_faults(
            RuntimeOrigin::signed(account(storage_provider)),
            DeclareFaultsParams {
                faults: faults.clone()
            },
        ));

        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();

        let mut updates = 0;

        for dl in sp.deadlines.due.iter() {
            for (_, partition) in dl.partitions.iter() {
                if partition.faults.len() > 0 {
                    updates += partition.faults.len();
                }
            }
        }
        // One partitions fault should be added.
        assert_eq!(updates, 5);
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(Event::FaultsDeclared {
                owner: account(storage_provider),
                faults
            })]
        );
    });
}

fn default_fault_setup(storage_provider: &str, storage_client: &str) {
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
    let sector_number = 1;

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
