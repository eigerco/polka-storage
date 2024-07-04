use core::str::FromStr;

use cid::Cid;
use frame_support::{
    assert_err, assert_noop, assert_ok,
    sp_runtime::{bounded_vec, ArithmeticError, DispatchError, TokenError},
};
use primitives_proofs::{
    ActiveDeal, ActiveSector, DealId, Market as MarketTrait, RegisteredSealProof, SectorDeal,
};
use sp_core::H256;

use crate::{
    mock::*, ActiveDealState, BalanceEntry, BalanceTable, DealProposal, DealSettlementError,
    DealState, DealsForBlock, Error, Event, PendingProposals, Proposals,
};

#[test]
fn initial_state() {
    new_test_ext().execute_with(|| {
        assert_eq!(Balances::free_balance(Market::account_id()), 0);
        assert_eq!(
            BalanceTable::<Test>::get(account(ALICE)),
            BalanceEntry::<u64> { free: 0, locked: 0 }
        );
    });
}

#[test]
fn adds_and_withdraws_balances() {
    new_test_ext().execute_with(|| {
        // Adds funds from an account to the Market
        assert_ok!(Market::add_balance(
            RuntimeOrigin::signed(account(ALICE)),
            10
        ));
        assert_eq!(Balances::free_balance(Market::account_id()), 10);
        assert_eq!(Balances::free_balance(account(ALICE)), INITIAL_FUNDS - 10);
        assert_eq!(
            BalanceTable::<Test>::get(account(ALICE)),
            BalanceEntry::<u64> {
                free: 10,
                locked: 0,
            }
        );

        // Is able to withdraw added funds back
        assert_ok!(Market::withdraw_balance(
            RuntimeOrigin::signed(account(ALICE)),
            10
        ));
        assert_eq!(Balances::free_balance(Market::account_id()), 0);
        assert_eq!(Balances::free_balance(account(ALICE)), INITIAL_FUNDS);
        assert_eq!(
            BalanceTable::<Test>::get(account(ALICE)),
            BalanceEntry::<u64> { free: 0, locked: 0 }
        );
    });
}

#[test]
fn adds_balance() {
    new_test_ext().execute_with(|| {
        assert_ok!(Market::add_balance(
            RuntimeOrigin::signed(account(ALICE)),
            10
        ));
        assert_eq!(Balances::free_balance(Market::account_id()), 10);
        assert_eq!(Balances::free_balance(account(ALICE)), INITIAL_FUNDS - 10);
        assert_eq!(
            BalanceTable::<Test>::get(account(ALICE)),
            BalanceEntry::<u64> {
                free: 10,
                locked: 0,
            }
        );

        assert_eq!(
            events(),
            [
                RuntimeEvent::System(frame_system::Event::<Test>::NewAccount {
                    account: Market::account_id()
                }),
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Endowed {
                    account: Market::account_id(),
                    free_balance: 10
                }),
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Transfer {
                    from: account(ALICE),
                    to: Market::account_id(),
                    amount: 10
                }),
                RuntimeEvent::Market(Event::<Test>::BalanceAdded {
                    who: account(ALICE),
                    amount: 10
                })
            ]
        );

        // Makes sure other accounts are unaffected
        assert_eq!(
            BalanceTable::<Test>::get(account(BOB)),
            BalanceEntry::<u64> { free: 0, locked: 0 }
        );
    });
}

#[test]
fn fails_to_add_balance_insufficient_funds() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Market::add_balance(RuntimeOrigin::signed(account(ALICE)), INITIAL_FUNDS + 1),
            TokenError::FundsUnavailable,
        );
    });
}

#[test]
fn fails_to_add_balance_overflow() {
    new_test_ext().execute_with(|| {
        // Hard to do this without setting it explicitly in the map
        BalanceTable::<Test>::set(
            account(BOB),
            BalanceEntry::<u64> {
                free: u64::MAX,
                locked: 0,
            },
        );

        assert_noop!(
            Market::add_balance(RuntimeOrigin::signed(account(BOB)), 1),
            ArithmeticError::Overflow
        );
    });
}

#[test]
fn withdraws_balance() {
    new_test_ext().execute_with(|| {
        let _ = Market::add_balance(RuntimeOrigin::signed(account(ALICE)), 10);
        System::reset_events();

        assert_ok!(Market::withdraw_balance(
            RuntimeOrigin::signed(account(ALICE)),
            10
        ));
        assert_eq!(Balances::free_balance(Market::account_id()), 0);
        assert_eq!(Balances::free_balance(account(ALICE)), INITIAL_FUNDS);
        assert_eq!(
            BalanceTable::<Test>::get(account(ALICE)),
            BalanceEntry::<u64> { free: 0, locked: 0 }
        );

        assert_eq!(
            events(),
            [
                RuntimeEvent::System(frame_system::Event::<Test>::KilledAccount {
                    account: Market::account_id()
                }),
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Transfer {
                    from: Market::account_id(),
                    to: account(ALICE),
                    amount: 10
                }),
                RuntimeEvent::Market(Event::<Test>::BalanceWithdrawn {
                    who: account(ALICE),
                    amount: 10
                })
            ]
        );
    });
}

#[test]
fn fails_to_withdraw_balance() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Market::withdraw_balance(RuntimeOrigin::signed(account(BOB)), 10),
            Error::<Test>::InsufficientFreeFunds
        );

        assert_eq!(events(), []);
    });
}

#[test]
fn publish_storage_deals_fails_with_empty_deals() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Market::publish_storage_deals(RuntimeOrigin::signed(account(PROVIDER)), bounded_vec![]),
            Error::<Test>::NoProposalsToBePublished
        );
    });
}

#[test]
fn publish_storage_deals() {
    let _ = env_logger::try_init();

    new_test_ext().execute_with(|| {
        let alice_start_block = 100;
        let alice_deal_id = 0;
        let alice_proposal = sign_proposal(
            ALICE,
            DealProposal {
                piece_cid: cid_of("polka-storage-data")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                piece_size: 18,
                client: account(ALICE),
                provider: account(PROVIDER),
                label: bounded_vec![0xb, 0xe, 0xe, 0xf],
                start_block: alice_start_block,
                end_block: 110,
                storage_price_per_block: 5,
                provider_collateral: 25,
                state: DealState::Published,
            },
        );
        let bob_start_block = 130;
        let bob_deal_id = 1;
        let bob_proposal = sign_proposal(
            BOB,
            DealProposal {
                piece_cid: cid_of("polka-storage-data-bob")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                piece_size: 21,
                client: account(BOB),
                provider: account(PROVIDER),
                label: bounded_vec![0xa, 0xe, 0xe, 0xf],
                start_block: bob_start_block,
                end_block: 135,
                storage_price_per_block: 10,
                provider_collateral: 15,
                state: DealState::Published,
            },
        );
        let alice_hash = Market::hash_proposal(&alice_proposal.proposal);
        let bob_hash = Market::hash_proposal(&bob_proposal.proposal);

        let _ = Market::add_balance(RuntimeOrigin::signed(account(ALICE)), 60);
        let _ = Market::add_balance(RuntimeOrigin::signed(account(BOB)), 70);
        let _ = Market::add_balance(RuntimeOrigin::signed(account(PROVIDER)), 75);
        System::reset_events();

        assert_ok!(Market::publish_storage_deals(
            RuntimeOrigin::signed(account(PROVIDER)),
            bounded_vec![alice_proposal, bob_proposal]
        ));
        assert_eq!(
            BalanceTable::<Test>::get(account(ALICE)),
            BalanceEntry::<u64> {
                free: 10,
                locked: 50
            }
        );
        assert_eq!(
            BalanceTable::<Test>::get(account(BOB)),
            BalanceEntry::<u64> {
                free: 20,
                locked: 50
            }
        );
        assert_eq!(
            BalanceTable::<Test>::get(account(PROVIDER)),
            BalanceEntry::<u64> {
                free: 35,
                locked: 40
            }
        );

        assert_eq!(
            events(),
            [
                RuntimeEvent::Market(Event::<Test>::DealPublished {
                    deal_id: alice_deal_id,
                    client: account(ALICE),
                    provider: account(PROVIDER),
                }),
                RuntimeEvent::Market(Event::<Test>::DealPublished {
                    deal_id: bob_deal_id,
                    client: account(BOB),
                    provider: account(PROVIDER),
                }),
            ]
        );
        assert!(PendingProposals::<Test>::get().contains(&alice_hash));
        assert!(PendingProposals::<Test>::get().contains(&bob_hash));
        assert!(DealsForBlock::<Test>::get(&alice_start_block).contains(&alice_deal_id));
        assert!(DealsForBlock::<Test>::get(&bob_start_block).contains(&bob_deal_id));
    });
}

#[test]
fn verify_deals_for_activation() {
    let _ = env_logger::try_init();
    new_test_ext().execute_with(|| {
        publish_for_activation(
            1,
            DealProposal {
                piece_cid: cid_of("polka-storage-data")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                piece_size: 18,
                client: account(ALICE),
                provider: account(PROVIDER),
                label: bounded_vec![0xb, 0xe, 0xe, 0xf],
                start_block: 100,
                end_block: 110,
                storage_price_per_block: 5,
                provider_collateral: 25,
                state: DealState::Published,
            },
        );

        let deals = bounded_vec![
            SectorDeal {
                sector_number: 1,
                sector_expiry: 120,
                sector_type: RegisteredSealProof::StackedDRG2KiBV1P1,
                deal_ids: bounded_vec![1]
            },
            SectorDeal {
                sector_number: 2,
                sector_expiry: 50,
                sector_type: RegisteredSealProof::StackedDRG2KiBV1P1,
                deal_ids: bounded_vec![]
            }
        ];

        assert_eq!(
            Ok(bounded_vec![
                Some(
                    Cid::from_str("bafk2bzaceajreoxfdcpdvitpvxm7vkpvcimlob5ejebqgqidjkz4qoug4q6zu")
                        .unwrap()
                ),
                None,
            ]),
            Market::verify_deals_for_activation(&account(PROVIDER), deals)
        );
    });
}

#[test]
fn activate_deals() {
    let _ = env_logger::try_init();
    new_test_ext().execute_with(|| {
        let alice_hash = publish_for_activation(
            1,
            DealProposal {
                piece_cid: cid_of("polka-storage-data")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                piece_size: 18,
                client: account(ALICE),
                provider: account(PROVIDER),
                label: bounded_vec![0xb, 0xe, 0xe, 0xf],
                start_block: 100,
                end_block: 110,
                storage_price_per_block: 5,
                provider_collateral: 25,
                state: DealState::Published,
            },
        );

        let deals = bounded_vec![
            SectorDeal {
                sector_number: 1,
                sector_expiry: 120,
                sector_type: RegisteredSealProof::StackedDRG2KiBV1P1,
                deal_ids: bounded_vec![1]
            },
            SectorDeal {
                sector_number: 2,
                sector_expiry: 50,
                sector_type: RegisteredSealProof::StackedDRG2KiBV1P1,
                deal_ids: bounded_vec![]
            }
        ];

        let piece_cid =
            Cid::from_str("bafk2bzacecg3xxc4f2ql2hreiuy767u6r72ekdz54k7luieknboaakhft5rgk")
                .unwrap();
        let placeholder_commd_cid =
            Cid::from_str("bafk2bzaceajreoxfdcpdvitpvxm7vkpvcimlob5ejebqgqidjkz4qoug4q6zu")
                .unwrap();
        assert_eq!(
            Ok(bounded_vec![
                ActiveSector {
                    active_deals: bounded_vec![ActiveDeal {
                        client: account(ALICE),
                        piece_cid: piece_cid,
                        piece_size: 18
                    }],
                    unsealed_cid: Some(placeholder_commd_cid),
                },
                ActiveSector {
                    active_deals: bounded_vec![],
                    unsealed_cid: None
                }
            ]),
            Market::activate_deals(&account(PROVIDER), deals, true)
        );
        assert!(!PendingProposals::<Test>::get().contains(&alice_hash));
    });
}

/// Creates a new deal and saves it in the Runtime Storage.
/// In addition to saving it to `Proposals::<T>` it also calculate's
/// it's hash and saves it to `PendingProposals::<T>`.
/// Behaves like `publish_storage_deals` without the validation and calling extrinsics.
fn publish_for_activation(deal_id: DealId, deal: DealProposalOf<Test>) -> H256 {
    let hash = Market::hash_proposal(&deal);
    let mut pending = PendingProposals::<Test>::get();
    pending.try_insert(hash).unwrap();
    PendingProposals::<Test>::set(pending);

    Proposals::<Test>::insert(deal_id, deal);
    hash
}

#[test]
fn verifies_deals_on_block_finalization() {
    let _ = env_logger::try_init();

    new_test_ext().execute_with(|| {
        let alice_start_block = 100;
        let alice_deal_id = 0;
        let alice_proposal = sign_proposal(
            ALICE,
            DealProposal {
                piece_cid: cid_of("polka-storage-data")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                piece_size: 18,
                client: account(ALICE),
                provider: account(PROVIDER),
                label: bounded_vec![0xb, 0xe, 0xe, 0xf],
                start_block: alice_start_block,
                end_block: 110,
                storage_price_per_block: 5,
                provider_collateral: 25,
                state: DealState::Published,
            },
        );
        let bob_start_block = 130;
        let bob_deal_id = 1;
        let bob_proposal = sign_proposal(
            BOB,
            DealProposal {
                piece_cid: cid_of("polka-storage-data-bob")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                piece_size: 21,
                client: account(BOB),
                provider: account(PROVIDER),
                label: bounded_vec![0xa, 0xe, 0xe, 0xf],
                start_block: bob_start_block,
                end_block: 135,
                storage_price_per_block: 10,
                provider_collateral: 15,
                state: DealState::Published,
            },
        );

        let _ = Market::add_balance(RuntimeOrigin::signed(account(ALICE)), 60);
        let _ = Market::add_balance(RuntimeOrigin::signed(account(BOB)), 70);
        let _ = Market::add_balance(RuntimeOrigin::signed(account(PROVIDER)), 75);
        let _ = Market::publish_storage_deals(
            RuntimeOrigin::signed(account(PROVIDER)),
            bounded_vec![alice_proposal, bob_proposal],
        );
        let _ = Market::activate_deals(
            &account(PROVIDER),
            bounded_vec![SectorDeal {
                sector_number: 1,
                sector_expiry: 200,
                sector_type: RegisteredSealProof::StackedDRG2KiBV1P1,
                deal_ids: bounded_vec![0]
            }],
            true,
        );
        System::reset_events();

        // Scenario: Activate Alice's Deal, forget to do that for Bob's.
        // Alice's balance before the hook
        assert_eq!(
            BalanceTable::<Test>::get(account(ALICE)),
            BalanceEntry::<u64> {
                free: 10,
                locked: 50
            }
        );
        // After Alice's block, nothing changes to the balance. It has been activated properly.
        run_to_block(alice_start_block + 1);
        assert!(!DealsForBlock::<Test>::get(&alice_start_block).contains(&alice_deal_id));
        assert_eq!(
            BalanceTable::<Test>::get(account(ALICE)),
            BalanceEntry::<u64> {
                free: 10,
                locked: 50
            }
        );

        // Balances before processing the hook
        assert_eq!(
            BalanceTable::<Test>::get(account(BOB)),
            BalanceEntry::<u64> {
                free: 20,
                locked: 50
            }
        );
        assert_eq!(
            BalanceTable::<Test>::get(account(PROVIDER)),
            BalanceEntry::<u64> {
                free: 35,
                locked: 40
            }
        );
        // After exceeding Bob's deal start_block,
        // Storage Provider should be slashed for Bob's amount and Bob refunded.
        run_to_block(bob_start_block + 1);
        assert_eq!(
            BalanceTable::<Test>::get(account(BOB)),
            BalanceEntry::<u64> {
                free: 70,
                locked: 0
            }
        );
        assert_eq!(
            BalanceTable::<Test>::get(account(PROVIDER)),
            BalanceEntry::<u64> {
                free: 35,
                // 40 (locked) - 15 (lost collateral) = 25
                locked: 25
            }
        );

        assert!(!DealsForBlock::<Test>::get(&bob_start_block).contains(&bob_deal_id));
    });
}

#[test]
fn settle_deal_payments_not_found() {
    let _ = env_logger::try_init();
    new_test_ext().execute_with(|| {
        assert_ok!(Market::settle_deal_payments(
            RuntimeOrigin::signed(account(ALICE)),
            bounded_vec!(0)
        ));

        assert_eq!(
            events(),
            [RuntimeEvent::Market(Event::<Test>::DealsSettled {
                successful: bounded_vec!(),
                unsuccessful: bounded_vec!((0, DealSettlementError::DealNotFound))
            })]
        )
    });
}

#[test]
fn settle_deal_payments_early() {
    let _ = env_logger::try_init();
    new_test_ext().execute_with(|| {
        let alice_proposal = sign_proposal(
            ALICE,
            DealProposal {
                piece_cid: cid_of("polka-storage-data")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                piece_size: 18,
                client: account(ALICE),
                provider: account(PROVIDER),
                label: bounded_vec![0xb, 0xe, 0xe, 0xf],
                start_block: 100,
                end_block: 110,
                storage_price_per_block: 5,
                provider_collateral: 25,
                state: DealState::Published,
            },
        );

        let _ = Market::add_balance(RuntimeOrigin::signed(account(ALICE)), 60);
        let _ = Market::add_balance(RuntimeOrigin::signed(account(PROVIDER)), 75);

        assert_ok!(Market::publish_storage_deals(
            RuntimeOrigin::signed(account(PROVIDER)),
            bounded_vec![alice_proposal]
        ));
        System::reset_events();

        assert_ok!(Market::settle_deal_payments(
            RuntimeOrigin::signed(account(ALICE)),
            bounded_vec!(0)
        ));

        assert_eq!(
            events(),
            [RuntimeEvent::Market(Event::<Test>::DealsSettled {
                successful: bounded_vec!(),
                unsuccessful: bounded_vec!((0, DealSettlementError::EarlySettlement))
            })]
        )
    });
}

#[test]
fn settle_deal_payments_published() {
    let _ = env_logger::try_init();
    new_test_ext().execute_with(|| {
        let alice_proposal = sign_proposal(
            ALICE,
            DealProposal {
                piece_cid: cid_of("polka-storage-data")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                piece_size: 18,
                client: account(ALICE),
                provider: account(PROVIDER),
                label: bounded_vec![0xb, 0xe, 0xe, 0xf],
                start_block: 0,
                end_block: 10,
                storage_price_per_block: 5,
                provider_collateral: 25,
                state: DealState::Published,
            },
        );

        let _ = Market::add_balance(RuntimeOrigin::signed(account(ALICE)), 60);
        let _ = Market::add_balance(RuntimeOrigin::signed(account(BOB)), 70);
        let _ = Market::add_balance(RuntimeOrigin::signed(account(PROVIDER)), 75);

        assert_ok!(Market::publish_storage_deals(
            RuntimeOrigin::signed(account(PROVIDER)),
            bounded_vec![alice_proposal]
        ));

        Proposals::<Test>::insert(
            1,
            DealProposal {
                piece_cid: cid_of("polka-storage-data-bob")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                piece_size: 21,
                client: account(BOB),
                provider: account(PROVIDER),
                label: bounded_vec![0xa, 0xe, 0xe, 0xf],
                start_block: 0,
                end_block: 10,
                storage_price_per_block: 10,
                provider_collateral: 15,
                state: DealState::Published,
            },
        );

        System::reset_events();

        assert_ok!(Market::settle_deal_payments(
            RuntimeOrigin::signed(account(ALICE)),
            bounded_vec!(0, 1)
        ));

        assert_eq!(
            events(),
            [RuntimeEvent::Market(Event::<Test>::DealsSettled {
                successful: bounded_vec!(0, 1),
                unsuccessful: bounded_vec!()
            })]
        )
    });
}

#[test]
fn settle_deal_payments_active_future_last_update() {
    let _ = env_logger::try_init();
    new_test_ext().execute_with(|| {
        let _ = Market::add_balance(RuntimeOrigin::signed(account(ALICE)), 60);
        let _ = Market::add_balance(RuntimeOrigin::signed(account(PROVIDER)), 75);

        Proposals::<Test>::insert(
            0,
            DealProposal {
                piece_cid: cid_of("polka-storage-data")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                piece_size: 18,
                client: account(ALICE),
                provider: account(PROVIDER),
                label: bounded_vec![0xb, 0xe, 0xe, 0xf],
                start_block: 0,
                end_block: 10,
                storage_price_per_block: 5,
                provider_collateral: 25,
                state: DealState::Active(ActiveDealState {
                    sector_number: 0,
                    sector_start_block: 0,
                    last_updated_block: Some(10),
                    slash_block: None,
                }),
            },
        );
        System::reset_events();

        assert_ok!(Market::settle_deal_payments(
            RuntimeOrigin::signed(account(ALICE)),
            bounded_vec!(0)
        ));

        assert_eq!(
            events(),
            [RuntimeEvent::Market(Event::<Test>::DealsSettled {
                successful: bounded_vec!(),
                unsuccessful: bounded_vec!((0, DealSettlementError::FutureLastUpdate))
            })]
        )
    });
}

#[test]
fn settle_deal_payments_active_corruption() {
    let _ = env_logger::try_init();
    new_test_ext().execute_with(|| {
        let _ = Market::add_balance(RuntimeOrigin::signed(account(ALICE)), 60);
        let _ = Market::add_balance(RuntimeOrigin::signed(account(PROVIDER)), 75);

        Proposals::<Test>::insert(
            0,
            DealProposal {
                piece_cid: cid_of("polka-storage-data")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                piece_size: 18,
                client: account(ALICE),
                provider: account(PROVIDER),
                label: bounded_vec![0xb, 0xe, 0xe, 0xf],
                start_block: 0,
                end_block: 10,
                storage_price_per_block: 5,
                provider_collateral: 25,
                state: DealState::Active(ActiveDealState {
                    sector_number: 0,
                    sector_start_block: 0,
                    last_updated_block: Some(11),
                    slash_block: None,
                }),
            },
        );
        run_to_block(12);
        System::reset_events();

        assert_err!(
            Market::settle_deal_payments(RuntimeOrigin::signed(account(ALICE)), bounded_vec!(0)),
            DispatchError::Corruption
        );

        assert_eq!(events(), [])
    });
}

#[test]
fn settle_deal_payments_success() {
    let _ = env_logger::try_init();
    new_test_ext().execute_with(|| {
        let alice_proposal = sign_proposal(
            ALICE,
            DealProposal {
                piece_cid: cid_of("polka-storage-data")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                piece_size: 18,
                client: account(ALICE),
                provider: account(PROVIDER),
                label: bounded_vec![0xb, 0xe, 0xe, 0xf],
                start_block: 0,
                end_block: 10,
                storage_price_per_block: 5,
                provider_collateral: 25,
                state: DealState::Published,
            },
        );

        let _ = Market::add_balance(RuntimeOrigin::signed(account(ALICE)), 60);
        let _ = Market::add_balance(RuntimeOrigin::signed(account(PROVIDER)), 75);

        assert_ok!(Market::publish_storage_deals(
            RuntimeOrigin::signed(account(PROVIDER)),
            bounded_vec![alice_proposal]
        ));

        Proposals::<Test>::mutate(0, |proposal| {
            if let Some(proposal) = proposal {
                proposal.state = DealState::Active(ActiveDealState {
                    sector_number: 0,
                    sector_start_block: 0,
                    last_updated_block: None,
                    slash_block: None,
                })
            }
        });

        assert_eq!(
            Proposals::<Test>::get(0),
            Some(DealProposal {
                piece_cid: cid_of("polka-storage-data")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                piece_size: 18,
                client: account(ALICE),
                provider: account(PROVIDER),
                label: bounded_vec![0xb, 0xe, 0xe, 0xf],
                start_block: 0,
                end_block: 10,
                storage_price_per_block: 5,
                provider_collateral: 25,
                state: DealState::Active(ActiveDealState {
                    sector_number: 0,
                    sector_start_block: 0,
                    last_updated_block: None,
                    slash_block: None,
                }),
            })
        );

        System::reset_events();

        run_to_block(5);

        assert_ok!(Market::settle_deal_payments(
            RuntimeOrigin::signed(account(ALICE)),
            bounded_vec!(0)
        ));

        assert_eq!(
            events(),
            [RuntimeEvent::Market(Event::<Test>::DealsSettled {
                successful: bounded_vec!(0),
                unsuccessful: bounded_vec!()
            })]
        );

        assert_eq!(
            BalanceTable::<Test>::get(account(PROVIDER)),
            BalanceEntry::<u64> {
                free: 75, // 50 (from 75 - collateral) + 5 * 5 (price per block * n blocks)
                locked: 25
            }
        );

        assert_eq!(
            BalanceTable::<Test>::get(account(ALICE)),
            BalanceEntry::<u64> {
                free: 10,
                locked: 25, // 50 - 5 * 5 (price per block * n blocks)
            }
        );

        assert_eq!(
            Proposals::<Test>::get(0),
            Some(DealProposal {
                piece_cid: cid_of("polka-storage-data")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                piece_size: 18,
                client: account(ALICE),
                provider: account(PROVIDER),
                label: bounded_vec![0xb, 0xe, 0xe, 0xf],
                start_block: 0,
                end_block: 10,
                storage_price_per_block: 5,
                provider_collateral: 25,
                state: DealState::Active(ActiveDealState {
                    sector_number: 0,
                    sector_start_block: 0,
                    last_updated_block: Some(5),
                    slash_block: None,
                }),
            })
        );
    });
}

#[test]
fn settle_deal_payments_success_finished() {
    let _ = env_logger::try_init();
    new_test_ext().execute_with(|| {
        let alice_proposal = sign_proposal(
            ALICE,
            DealProposal {
                piece_cid: cid_of("polka-storage-data")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                piece_size: 18,
                client: account(ALICE),
                provider: account(PROVIDER),
                label: bounded_vec![0xb, 0xe, 0xe, 0xf],
                start_block: 0,
                end_block: 10,
                storage_price_per_block: 5,
                provider_collateral: 25,
                state: DealState::Published,
            },
        );

        let _ = Market::add_balance(RuntimeOrigin::signed(account(ALICE)), 60);
        let _ = Market::add_balance(RuntimeOrigin::signed(account(PROVIDER)), 75);

        assert_ok!(Market::publish_storage_deals(
            RuntimeOrigin::signed(account(PROVIDER)),
            bounded_vec![alice_proposal]
        ));

        Proposals::<Test>::mutate(0, |proposal| {
            if let Some(proposal) = proposal {
                proposal.state = DealState::Active(ActiveDealState {
                    sector_number: 0,
                    sector_start_block: 0,
                    last_updated_block: None,
                    slash_block: None,
                })
            }
        });

        assert_eq!(
            Proposals::<Test>::get(0),
            Some(DealProposal {
                piece_cid: cid_of("polka-storage-data")
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes"),
                piece_size: 18,
                client: account(ALICE),
                provider: account(PROVIDER),
                label: bounded_vec![0xb, 0xe, 0xe, 0xf],
                start_block: 0,
                end_block: 10,
                storage_price_per_block: 5,
                provider_collateral: 25,
                state: DealState::Active(ActiveDealState {
                    sector_number: 0,
                    sector_start_block: 0,
                    last_updated_block: None,
                    slash_block: None,
                }),
            })
        );

        System::reset_events();

        // Deal is finished
        run_to_block(11);

        assert_ok!(Market::settle_deal_payments(
            RuntimeOrigin::signed(account(ALICE)),
            bounded_vec!(0)
        ));

        assert_eq!(
            events(),
            [RuntimeEvent::Market(Event::<Test>::DealsSettled {
                successful: bounded_vec!(0),
                unsuccessful: bounded_vec!()
            })]
        );

        assert_eq!(
            BalanceTable::<Test>::get(account(PROVIDER)),
            BalanceEntry::<u64> {
                free: 50 + 5 * 10, // 50 (from 75 - collateral) + (price per block * n blocks)
                locked: 25
            }
        );

        assert_eq!(
            BalanceTable::<Test>::get(account(ALICE)),
            BalanceEntry::<u64> {
                free: 10,
                locked: 50 - 5 * 10, // locked - (price per block * n blocks)
            }
        );

        assert_eq!(Proposals::<Test>::get(0), None);
    });
}
