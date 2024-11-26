extern crate alloc;

use alloc::collections::{BTreeMap, BTreeSet};

use primitives_proofs::SectorNumber;
use sp_core::bounded_vec;

use super::new_test_ext;
use crate::{
    pallet::{Event, StorageProviders},
    sector::ProveCommitSector,
    tests::{
        account, events, publish_deals, register_storage_provider, run_to_block, sector_set,
        Balances, RuntimeEvent, RuntimeOrigin, SectorPreCommitInfoBuilder, StorageProvider, System,
        Test, CHARLIE,
    },
};

/// Publish 2 deals, by a 1 Storage Provider.
/// Precommit both of them, but prove only the 2nd one.
/// First one should be slashed -> pre_commit_deposit slashed & burned and removed from state + emitted event.
/// Second one should **NOT** be slashed -> just removed during proving and not touched by the hook.
/// There is a balance in pre_commit_deposit after proving, because we release balance after termination.
#[test]
fn pre_commit_hook_slashed_deal() {
    new_test_ext().execute_with(|| {
        let storage_provider = CHARLIE;
        register_storage_provider(account(storage_provider));
        publish_deals(storage_provider);
        let first_deal = 0;
        let second_deal = 1;
        // TODO(@aidan46, #106, 2024-06-24): Set a logical value or calculation
        let deal_precommit_deposit = 1;

        let first_sector = SectorPreCommitInfoBuilder::default()
            .sector_number(1.into())
            .deals(bounded_vec![first_deal])
            .build();
        // First sector will not be proven, that's why we split deals across sectors
        let second_sector = SectorPreCommitInfoBuilder::default()
            .deals(bounded_vec![second_deal])
            .sector_number(2.into())
            .build();

        StorageProvider::pre_commit_sectors(
            RuntimeOrigin::signed(account(storage_provider)),
            bounded_vec![first_sector.clone()],
        )
        .unwrap();
        StorageProvider::pre_commit_sectors(
            RuntimeOrigin::signed(account(storage_provider)),
            bounded_vec![second_sector.clone()],
        )
        .unwrap();

        StorageProvider::prove_commit_sectors(
            RuntimeOrigin::signed(account(storage_provider)),
            bounded_vec![ProveCommitSector {
                sector_number: 2.into(),
                proof: bounded_vec![0xde],
            }],
        )
        .unwrap();

        System::reset_events();

        // Running to block after it should have been slashed.
        // It wouldn't if we had proven it before.
        run_to_block(first_sector.expiration + 1);

        let sp = StorageProviders::<Test>::get(account(storage_provider))
            .expect("SP should be present because of the pre-check");
        assert!(sp.sectors.contains_key(&second_sector.sector_number));
        // First sector removed from here because it was slashed, second one because it was proven.
        assert!(sp.pre_committed_sectors.is_empty());
        // Pre-commit from the second deal is still there, as pre-commit deposits are until sector expired.
        assert_eq!(sp.pre_commit_deposits, deal_precommit_deposit);
        assert_eq!(
            Balances::reserved_balance(account(storage_provider)),
            deal_precommit_deposit
        );
        assert_eq!(
            events(),
            [
                RuntimeEvent::StorageProvider(Event::<Test>::PartitionsFaulty {
                    owner: account(storage_provider),
                    faulty_partitions: BTreeMap::from([(
                        0,
                        BTreeSet::from([SectorNumber::new(2).unwrap()])
                    )]),
                }),
                RuntimeEvent::StorageProvider(Event::<Test>::SectorSlashed {
                    owner: account(storage_provider),
                    sector_number: 1.into(),
                }),
                // the slash -> withdraw is related to the usage of slash_and_burn
                // when slashing the SP for a failed pre_commit
                // this usage may need review for a proper economic balance
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Slashed {
                    who: account(storage_provider),
                    amount: deal_precommit_deposit,
                }),
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Rescinded {
                    amount: deal_precommit_deposit
                }),
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Withdraw {
                    who: account(storage_provider),
                    amount: deal_precommit_deposit,
                }),
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Rescinded {
                    amount: deal_precommit_deposit
                }),
            ]
        );
    });
}
