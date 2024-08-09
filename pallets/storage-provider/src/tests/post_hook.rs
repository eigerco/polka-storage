use sp_core::bounded_vec;
use frame_support::{
    assert_err, assert_noop, assert_ok,
    pallet_prelude::{ConstU32, Get},
};

use super::new_test_ext;
use crate::{
    pallet::{Config, Event, StorageProviders},
    sector::ProveCommitSector,
    tests::{
        account, events, publish_deals, register_storage_provider, run_to_block, Balances,
        RuntimeEvent, RuntimeOrigin, SectorPreCommitInfoBuilder, StorageProvider, System, Test,
        CHARLIE,
    },
};

/// Publish 2 deals, by a 1 Storage Provider.
/// Precommit both of them, prove both of them.

#[test]
fn post_commit_hook_slashed_deal() {
    new_test_ext().execute_with(|| {
        let storage_provider = CHARLIE;
        register_storage_provider(account(storage_provider));
        publish_deals(storage_provider);
        let first_deal = 0;
        let second_deal = 1;
        // TODO(@aidan46, #106, 2024-06-24): Set a logical value or calculation
        let deal_precommit_deposit = 1;

        let first_sector = SectorPreCommitInfoBuilder::default()
            .sector_number(1)
            .deals(bounded_vec![first_deal])
            .build();
        // First sector will not be proven, that's why we split deals across sectors
        let second_sector = SectorPreCommitInfoBuilder::default()
            .deals(bounded_vec![second_deal])
            .sector_number(2)
            .build();

        StorageProvider::pre_commit_sector(
            RuntimeOrigin::signed(account(storage_provider)),
            first_sector.clone(),
        )
        .unwrap();
        StorageProvider::pre_commit_sector(
            RuntimeOrigin::signed(account(storage_provider)),
            second_sector.clone(),
        )
        .unwrap();

        StorageProvider::prove_commit_sector(
            RuntimeOrigin::signed(account(storage_provider)),
            ProveCommitSector {
                sector_number: 1,
                proof: bounded_vec![0xde],
            },
        )
        .unwrap();

        StorageProvider::prove_commit_sector(
            RuntimeOrigin::signed(account(storage_provider)),
            ProveCommitSector {
                sector_number: 2,
                proof: bounded_vec![0xde],
            },
        )
        .unwrap();

        System::reset_events();

        let sp = StorageProviders::<Test>::get(account(storage_provider))
            .expect("SP should be present because of the pre-check");
        let assigned_deadline_end = sp.proving_period_start + <<Test as Config>::WPoStChallengeWindow as Get<u64>>::get();

        log::warn!("running until: {}", assigned_deadline_end + 1);
        run_to_block(assigned_deadline_end + 1);


        /* log::warn!("proving period: {}", sp.proving_period_start);
        for (idx, deadline) in sp.deadlines.due.iter().enumerate() {
            log::warn!("checking deadline: {} sectors: {} partitions{}", idx, deadline.live_sectors, deadline.partitions.len());
        } */
    });
}
