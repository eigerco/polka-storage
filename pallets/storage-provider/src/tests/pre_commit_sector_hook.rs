use primitives::sector::{ProveCommitSector, SectorNumber};
use sp_core::bounded_vec;
use sp_runtime::{BoundedBTreeMap, BoundedBTreeSet};

use super::{new_test_ext, Balances};
use crate::{
    pallet::{Event, StorageProviders},
    tests::{
        account, events, publish_deals, register_storage_provider, run_to_block, RuntimeEvent,
        RuntimeOrigin, SectorPreCommitInfoBuilder, StorageProvider, System, Test, ALICE, CHARLIE,
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
        // TODO(@aidan46, #106, 2024-06-24): Set a logical value or calculation
        const DEAL_PRECOMMIT_DEPOSIT: u64 = 1;
        const DEAL_COLLATERAL: u64 = 100;

        let storage_provider = CHARLIE;
        register_storage_provider(account(storage_provider));
        publish_deals(storage_provider);
        let first_deal = 0;
        let second_deal = 1;

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
        // 2 deals = (collateral + precommit) * 2
        assert_eq!(
            Balances::reserved_balance(&account(storage_provider)),
            2 * (DEAL_COLLATERAL + DEAL_PRECOMMIT_DEPOSIT)
        );

        StorageProvider::prove_commit_sectors(
            RuntimeOrigin::signed(account(storage_provider)),
            bounded_vec![ProveCommitSector {
                sector_number: 2.into(),
                proofs: bounded_vec![bounded_vec![0xde]],
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
        // 1 deal got slashed so the respective locked funds *vanished*
        assert_eq!(
            Balances::reserved_balance(&account(storage_provider)),
            DEAL_COLLATERAL
        );
        let mut expected_faulty_sectors = BoundedBTreeSet::new();
        expected_faulty_sectors
            .try_insert(SectorNumber::new(2).unwrap())
            .unwrap();
        let mut expected_faulty_partitions = BoundedBTreeMap::new();
        expected_faulty_partitions
            .try_insert(0, expected_faulty_sectors)
            .unwrap();
        assert_eq!(
            events(),
            [
                RuntimeEvent::StorageProvider(Event::<Test>::PartitionsFaulty {
                    owner: account(storage_provider),
                    faulty_partitions: expected_faulty_partitions,
                }),
                RuntimeEvent::StorageProvider(Event::DealSlashed {
                    deal_id: 0,
                    amount: DEAL_COLLATERAL,
                    client: account(ALICE),
                    provider: account(storage_provider)
                }),
                RuntimeEvent::StorageProvider(Event::<Test>::SectorsSlashed {
                    owner: account(storage_provider),
                    sector_numbers: bounded_vec![1.into()],
                }),
            ]
        );
    });
}
