use frame_support::{assert_ok, BoundedBTreeSet};
use sp_core::bounded_vec;

use crate::{
    fault::{DeclareFaultsParams, FaultDeclaration},
    pallet::StorageProviders,
    sector::ProveCommitSector,
    tests::{
        account, new_test_ext, register_storage_provider, DealProposalBuilder, Market,
        RuntimeOrigin, SectorPreCommitInfoBuilder, StorageProvider, Test, ALICE, BOB,
    },
};

#[test]
fn declare_faults() {
    new_test_ext().execute_with(|| {
        // Setup accounts
        let storage_provider = ALICE;
        let storage_client = BOB;

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

        let mut sectors = BoundedBTreeSet::new();
        sectors.try_insert(1).expect("Programmer error");
        let fault = FaultDeclaration {
            deadline: 1,
            partition: 1,
            sectors,
        };
        assert_ok!(StorageProvider::declare_faults(
            RuntimeOrigin::signed(account(storage_provider)),
            DeclareFaultsParams {
                faults: vec![fault]
            },
        ));

        let sp = StorageProviders::<Test>::get(account(storage_provider)).unwrap();

        let mut updates = 0;

        for (deadline_idx, dl) in sp.deadlines.due.iter().enumerate() {
            for (partition_number, partition) in dl.partitions.iter() {
                if partition.faults.len() > 0 {
                    log::info!("deadline[{deadline_idx}]; partition number {partition_number} = {partition:?}");
                    updates += 1;
                }
            }
        }
        // One partitions fault should be added.
        assert_eq!(updates, 1);
    });
}
