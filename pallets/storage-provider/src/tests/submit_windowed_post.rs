use frame_support::{assert_noop, assert_ok};
use rstest::rstest;
use sp_core::bounded_vec;
use sp_runtime::DispatchError;

use crate::{
    deadline::DeadlineError,
    pallet::{Error, Event, StorageProviders},
    sector::ProveCommitSector,
    tests::{
        account, events, new_test_ext, register_storage_provider, run_to_block,
        DealProposalBuilder, Market, RuntimeEvent, RuntimeOrigin, SectorPreCommitInfoBuilder,
        StorageProvider, SubmitWindowedPoStBuilder, System, Test, WpostChallengeWindow, ALICE, BOB,
    },
    Config,
};

/// Setup the environment for the submit_windowed_post tests
fn setup() {
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

    // Remove any events that were triggered until now.
    System::reset_events();

    // Check if the sector was successfully committed
    let state = StorageProviders::<Test>::get(account(ALICE)).unwrap();
    let new_dl = state.deadlines.due.first().expect("programmer error");
    assert_eq!(new_dl.live_sectors, 1);
    assert_eq!(new_dl.total_sectors, 1);
}

#[test]
fn fails_should_be_signed() {
    new_test_ext().execute_with(|| {
        // Build window post proof
        let windowed_post = SubmitWindowedPoStBuilder::default()
            .chain_commit_block(System::block_number() - 1) // Wrong proof
            .build();

        assert_noop!(
            StorageProvider::submit_windowed_post(RuntimeOrigin::none(), windowed_post),
            DispatchError::BadOrigin
        );
    });
}

#[test]
fn submit_windowed_post() {
    new_test_ext().execute_with(|| {
        setup();

        let partition = 0;

        // Run to block where the window post proof is to be submitted
        let proving_period_start = StorageProviders::<Test>::get(account(ALICE))
            .unwrap()
            .proving_period_start;
        run_to_block(proving_period_start);

        // Done with setup build window post proof
        let windowed_post = SubmitWindowedPoStBuilder::default()
            .chain_commit_block(System::block_number() - 1)
            .partition(partition)
            .build();

        // Run extrinsic and assert that the result is `Ok`
        assert_ok!(StorageProvider::submit_windowed_post(
            RuntimeOrigin::signed(account(ALICE)),
            windowed_post,
        ));

        // Check that expected events were emitted
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(
                Event::<Test>::ValidPoStSubmitted {
                    owner: account(ALICE)
                }
            )]
        );

        let state = StorageProviders::<Test>::get(account(ALICE)).unwrap();
        let deadlines = state.deadlines;
        let new_dl = deadlines.due.first().expect("Programmer error");
        let posted_partition = new_dl.partitions_posted.get(&partition).copied();

        assert_eq!(posted_partition, Some(partition));
    });
}

#[test]
fn submit_windowed_post_for_sector_twice() {
    new_test_ext().execute_with(|| {
        setup();

        // Run to block where the window post proof is to be submitted
        let proving_period_start = StorageProviders::<Test>::get(account(ALICE))
            .unwrap()
            .proving_period_start;
        run_to_block(proving_period_start);

        // Build window post proof
        let windowed_post = SubmitWindowedPoStBuilder::default()
            .partition(0)
            .chain_commit_block(System::block_number() - 1)
            .build();

        // Run extrinsic and assert that the result is `Ok`
        assert_ok!(StorageProvider::submit_windowed_post(
            RuntimeOrigin::signed(account(ALICE)),
            windowed_post.clone(),
        ));
        // Check that only one event was emitted
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(
                Event::<Test>::ValidPoStSubmitted {
                    owner: account(ALICE)
                }
            )]
        );

        // Run extrinsic and assert that the result is `Err`
        assert_noop!(
            StorageProvider::submit_windowed_post(
                RuntimeOrigin::signed(account(ALICE)),
                windowed_post,
            ),
            Error::<Test>::DeadlineError(DeadlineError::PartitionAlreadyProven)
        );
        // Check that nothing was emitted
        assert_eq!(events(), []);
    });
}

#[test]
fn should_fail_before_first_post() {
    new_test_ext().execute_with(|| {
        setup();

        // Run to block where the window post proof is to be submitted
        run_to_block(19);

        // Build window post proof
        let windowed_post = SubmitWindowedPoStBuilder::default()
            .chain_commit_block(System::block_number() - 1)
            .partition(2) // This partition does not exist
            .build();

        // Run extrinsic
        assert_noop!(
            StorageProvider::submit_windowed_post(
                RuntimeOrigin::signed(account(ALICE)),
                windowed_post,
            ),
            Error::<Test>::InvalidDeadlineSubmission
        );
    });
}

#[test]
fn should_fail_when_proving_wrong_partition() {
    new_test_ext().execute_with(|| {
        setup();

        // Run to block where the window post proof is to be submitted
        let proving_period_start = StorageProviders::<Test>::get(account(ALICE))
            .unwrap()
            .proving_period_start;
        run_to_block(proving_period_start);

        // Build window post proof
        let windowed_post = SubmitWindowedPoStBuilder::default()
            .chain_commit_block(System::block_number() - 1)
            .partition(2) // This partition does not exist
            .build();

        // Run extrinsic
        assert_noop!(
            StorageProvider::submit_windowed_post(
                RuntimeOrigin::signed(account(ALICE)),
                windowed_post,
            ),
            Error::<Test>::DeadlineError(DeadlineError::PartitionNotFound)
        );
    });
}

#[test]
fn fail_windowed_post_deadline_not_opened() {
    new_test_ext().execute_with(|| {
        setup();

        // Run to block where the window post proof is to be submitted
        let proving_period_start = StorageProviders::<Test>::get(account(ALICE))
            .unwrap()
            .proving_period_start;
        run_to_block(proving_period_start + <Test as Config>::WPoStChallengeWindow::get() * 3 - 1);

        // Build window post proof
        let windowed_post = SubmitWindowedPoStBuilder::default()
            .chain_commit_block(System::block_number() - 1)
            .build();

        // Run extrinsic
        assert_noop!(
            StorageProvider::submit_windowed_post(
                RuntimeOrigin::signed(account(ALICE)),
                windowed_post,
            ),
            Error::<Test>::InvalidDeadlineSubmission
        );
    });
}

#[test]
fn fail_windowed_post_wrong_deadline_index_used() {
    new_test_ext().execute_with(|| {
        setup();

        // Run to block where the window post proof is to be submitted
        let proving_period_start = StorageProviders::<Test>::get(account(ALICE))
            .unwrap()
            .proving_period_start;
        run_to_block(proving_period_start);

        // Build window post proof
        let windowed_post = SubmitWindowedPoStBuilder::default()
            .deadline(1) // This index is wrong because it is specifying the next deadline that will be opened
            .chain_commit_block(System::block_number() - 1)
            .build();

        // Run extrinsic
        assert_noop!(
            StorageProvider::submit_windowed_post(
                RuntimeOrigin::signed(account(ALICE)),
                windowed_post,
            ),
            Error::<Test>::InvalidDeadlineSubmission
        );
    });
}

#[test]
fn fail_windowed_post_wrong_signature() {
    new_test_ext().execute_with(|| {
        setup();

        // Run to block where the window post proof is to be submitted
        let proving_period_start = StorageProviders::<Test>::get(account(ALICE))
            .unwrap()
            .proving_period_start;
        run_to_block(proving_period_start);

        // Build window post proof
        let windowed_post = SubmitWindowedPoStBuilder::default()
            .chain_commit_block(System::block_number() - 1)
            .proof_bytes(vec![]) // Wrong proof
            .build();

        // Run extrinsic
        assert_noop!(
            StorageProvider::submit_windowed_post(
                RuntimeOrigin::signed(account(ALICE)),
                windowed_post,
            ),
            Error::<Test>::PoStProofInvalid
        );
    });
}

#[test]
fn fail_windowed_post_future_commit_block() {
    new_test_ext().execute_with(|| {
        setup();

        // Run to block where the window post proof is to be submitted
        let proving_period_start = StorageProviders::<Test>::get(account(ALICE))
            .unwrap()
            .proving_period_start;
        run_to_block(proving_period_start);

        // Build window post proof
        let windowed_post = SubmitWindowedPoStBuilder::default()
            .deadline(0)
            .chain_commit_block(System::block_number()) // Our block commitment should be in the past
            .build();

        // Run extrinsic
        assert_noop!(
            StorageProvider::submit_windowed_post(
                RuntimeOrigin::signed(account(ALICE)),
                windowed_post,
            ),
            Error::<Test>::PoStProofInvalid
        );
    });
}

#[rstest]
// deadline is not opened
#[case(-9, -10, Err(Error::<Test>::InvalidDeadlineSubmission.into()))]
// deadline is opened. commit block is on the current block
#[case(0, 0, Err(Error::<Test>::PoStProofInvalid.into()))]
// commit block is set on the block before the deadline is officially opened
#[case(0, -1, Ok(()))]
// submit proof on the last allowed block for the deadline
#[case(1, 0, Ok(()))]
// deadline has passed
#[case(2, 1, Err(Error::<Test>::InvalidDeadlineSubmission.into()))]
fn windowed_post_commit_block(
    #[case] block_offset: i64,
    #[case] chain_commit_block_offset: i64,
    #[case] expected_extrinsic_result: Result<(), DispatchError>,
) {
    new_test_ext().execute_with(|| {
        // Setup environment
        setup();

        // Run to block where the window post proof is to be submitted
        let proving_period_start = StorageProviders::<Test>::get(account(ALICE))
            .unwrap()
            .proving_period_start;

        let target_block = ((proving_period_start as i64) + block_offset) as u64;
        let chain_commit_block = ((proving_period_start as i64) + chain_commit_block_offset) as u64;

        log::error!("{} {}", target_block, chain_commit_block);

        // Cast is safe for this test, CANNOT be generalized for other uses
        run_to_block(target_block);

        // Build window post proof
        let windowed_post = SubmitWindowedPoStBuilder::default()
            .chain_commit_block(chain_commit_block)
            .partition(0)
            .build();

        // Run extrinsic
        assert_eq!(
            StorageProvider::submit_windowed_post(
                RuntimeOrigin::signed(account(ALICE)),
                windowed_post
            ),
            expected_extrinsic_result
        );
    });
}
