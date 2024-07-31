use frame_support::{assert_noop, assert_ok};
use sp_core::bounded_vec;
use sp_runtime::DispatchError;

use crate::{
    pallet::{Error, Event, StorageProviders},
    sector::ProveCommitSector,
    tests::{
        account, events, new_test_ext, register_storage_provider, run_to_block,
        DealProposalBuilder, Market, RuntimeError, RuntimeEvent, RuntimeOrigin,
        SectorPreCommitInfoBuilder, StorageProvider, SubmitWindowedPoStBuilder, System, Test,
        ALICE, BOB,
    },
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
}

// TODO: Remove ignore after the deadline calculation is fixed
#[ignore]
#[test]
fn successful_submit_windowed_post() {
    new_test_ext().execute_with(|| {
        setup();

        struct ProofSubmitted {
            deadline: u64,
            height: u64,
        }

        let submit_proofs = vec![
            ProofSubmitted {
                deadline: 0,
                height: 6700,
            },
            ProofSubmitted {
                deadline: 1,
                height: 13500,
            },
        ];

        for proof in submit_proofs {
            // Run to block where the window post proof is to be submitted
            run_to_block(proof.height);

            // Build window post proof
            let windowed_post = SubmitWindowedPoStBuilder::default()
                .deadline(proof.deadline)
                .chain_commit_block(System::block_number() - 1)
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
        }

        let state = StorageProviders::<Test>::get(account(ALICE)).unwrap();
        let deadlines = state.deadlines;
        let new_dl = deadlines.due.first().expect("programmer error");
        assert_eq!(new_dl.live_sectors, 1);
        assert_eq!(new_dl.total_sectors, 1);
    });
}

#[test]
fn fails_should_be_signed() {
    new_test_ext().execute_with(|| {
        setup();

        // Run to block where the window post proof is to be submitted
        run_to_block(6700);

        // Build window post proof
        let windowed_post = SubmitWindowedPoStBuilder::default()
            .deadline(0)
            .chain_commit_block(System::block_number() - 1) // Wrong proof
            .build();

        assert_noop!(
            StorageProvider::submit_windowed_post(RuntimeOrigin::none(), windowed_post,),
            DispatchError::BadOrigin
        );
    });
}

#[test]
fn fail_windowed_post_wrong_signature() {
    new_test_ext().execute_with(|| {
        setup();

        // Run to block where the window post proof is to be submitted
        run_to_block(6700);

        // Build window post proof
        let windowed_post = SubmitWindowedPoStBuilder::default()
            .deadline(0)
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
        run_to_block(6700);

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

#[test]
fn fail_windowed_post_deadline_not_opened() {
    new_test_ext().execute_with(|| {
        setup();

        // Build window post proof
        let windowed_post = SubmitWindowedPoStBuilder::default()
            .deadline(0)
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
        run_to_block(6700);

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

#[ignore]
#[test]
fn fail_windowed_post_commit_block_outside_challenge() {
    // TODO: Check that if we try to post a proof for the block outside the
    // challenge window, it should fail.
}
