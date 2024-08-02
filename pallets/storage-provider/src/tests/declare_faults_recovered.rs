use frame_support::{assert_ok, BoundedBTreeSet};

use crate::{
    fault::{
        DeclareFaultsParams, DeclareFaultsRecoveredParams, FaultDeclaration, RecoveryDeclaration,
    },
    pallet::{Event, StorageProviders},
    tests::{
        account, declare_faults::default_fault_setup, events, new_test_ext, RuntimeEvent,
        RuntimeOrigin, StorageProvider, Test, ALICE, BOB,
    },
};

#[test]
fn declare_single_fault_recovered() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;

        default_fault_setup(storage_provider, storage_client);
        let deadline = 1;
        let partition = 1;

        let mut sectors = BoundedBTreeSet::new();
        sectors.try_insert(1).expect("Programmer error");
        let fault = FaultDeclaration {
            deadline,
            partition,
            sectors: sectors.clone(),
        };
        assert_ok!(StorageProvider::declare_faults(
            RuntimeOrigin::signed(account(storage_provider)),
            DeclareFaultsParams {
                faults: vec![fault]
            },
        ));

        // Flush events
        events();

        // setup recovery
        let recovery = RecoveryDeclaration {
            deadline,
            partition,
            sectors,
        };

        // run extrinsic
        assert_ok!(StorageProvider::declare_faults_recovered(
            RuntimeOrigin::signed(account(storage_provider)),
            DeclareFaultsRecoveredParams {
                recoveries: vec![recovery.clone()]
            }
        ));

        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();

        let mut recoveries = 0;
        for dl in sp.deadlines.due.iter() {
            for (_, partition) in dl.partitions.iter() {
                if partition.recoveries.len() > 0 {
                    recoveries += 1;
                }
            }
        }

        // One partitions recovery should be added.
        assert_eq!(recoveries, 1);
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(Event::FaultsRecovered {
                owner: account(storage_provider),
                recoveries: vec![recovery]
            })]
        );
    });
}
