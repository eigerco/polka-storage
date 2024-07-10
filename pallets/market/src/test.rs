use core::str::FromStr;

use cid::Cid;
use frame_support::{
    assert_err, assert_noop, assert_ok,
    pallet_prelude::{ConstU32, Get},
    sp_runtime::{bounded_vec, ArithmeticError, DispatchError, TokenError},
    traits::Currency,
    BoundedVec,
};
use primitives_proofs::{
    ActiveDeal, ActiveSector, DealId, Market as MarketTrait, RegisteredSealProof, SectorDeal,
    MAX_DEALS_PER_SECTOR,
};
use sp_core::H256;
use sp_runtime::AccountId32;

use crate::{
    mock::*,
    pallet::{lock_funds, slash_and_burn, unlock_funds},
    ActiveDealState, BalanceEntry, BalanceTable, Config, DealSettlementError, DealState,
    DealsForBlock, Error, Event, PendingProposals, Proposals, SectorDeals, SectorTerminateError,
};
#[test]
fn initial_state() {
    new_test_ext().execute_with(|| {
        assert_eq!(Balances::free_balance(Market::account_id()), 0);
        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(ALICE)),
            BalanceEntry::<u64> { free: 0, locked: 0 }
        );
    });
}

#[test]
fn adds_and_withdraws_balances() {
    new_test_ext().execute_with(|| {
        // Adds funds from an account to the Market
        assert_ok!(Market::add_balance(
            RuntimeOrigin::signed(account::<Test>(ALICE)),
            10
        ));
        assert_eq!(Balances::free_balance(Market::account_id()), 10);
        assert_eq!(
            Balances::free_balance(account::<Test>(ALICE)),
            INITIAL_FUNDS - 10
        );
        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(ALICE)),
            BalanceEntry::<u64> {
                free: 10,
                locked: 0,
            }
        );

        // Is able to withdraw added funds back
        assert_ok!(Market::withdraw_balance(
            RuntimeOrigin::signed(account::<Test>(ALICE)),
            10
        ));
        assert_eq!(Balances::free_balance(Market::account_id()), 0);
        assert_eq!(
            Balances::free_balance(account::<Test>(ALICE)),
            INITIAL_FUNDS
        );
        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(ALICE)),
            BalanceEntry::<u64> { free: 0, locked: 0 }
        );
    });
}

#[test]
fn adds_balance() {
    new_test_ext().execute_with(|| {
        assert_ok!(Market::add_balance(
            RuntimeOrigin::signed(account::<Test>(ALICE)),
            10
        ));
        assert_eq!(Balances::free_balance(Market::account_id()), 10);
        assert_eq!(
            Balances::free_balance(account::<Test>(ALICE)),
            INITIAL_FUNDS - 10
        );
        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(ALICE)),
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
                    from: account::<Test>(ALICE),
                    to: Market::account_id(),
                    amount: 10
                }),
                RuntimeEvent::Market(Event::<Test>::BalanceAdded {
                    who: account::<Test>(ALICE),
                    amount: 10
                })
            ]
        );

        // Makes sure other accounts are unaffected
        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(BOB)),
            BalanceEntry::<u64> { free: 0, locked: 0 }
        );
    });
}

#[test]
fn fails_to_add_balance_insufficient_funds() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Market::add_balance(
                RuntimeOrigin::signed(account::<Test>(ALICE)),
                INITIAL_FUNDS + 1
            ),
            TokenError::FundsUnavailable,
        );
    });
}

#[test]
fn fails_to_add_balance_overflow() {
    new_test_ext().execute_with(|| {
        // Hard to do this without setting it explicitly in the map
        BalanceTable::<Test>::set(
            account::<Test>(BOB),
            BalanceEntry::<u64> {
                free: u64::MAX,
                locked: 0,
            },
        );

        assert_noop!(
            Market::add_balance(RuntimeOrigin::signed(account::<Test>(BOB)), 1),
            ArithmeticError::Overflow
        );
    });
}

#[test]
fn withdraws_balance() {
    new_test_ext().execute_with(|| {
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(ALICE)), 10);
        System::reset_events();

        assert_ok!(Market::withdraw_balance(
            RuntimeOrigin::signed(account::<Test>(ALICE)),
            10
        ));
        assert_eq!(Balances::free_balance(Market::account_id()), 0);
        assert_eq!(
            Balances::free_balance(account::<Test>(ALICE)),
            INITIAL_FUNDS
        );
        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(ALICE)),
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
                    to: account::<Test>(ALICE),
                    amount: 10
                }),
                RuntimeEvent::Market(Event::<Test>::BalanceWithdrawn {
                    who: account::<Test>(ALICE),
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
            Market::withdraw_balance(RuntimeOrigin::signed(account::<Test>(BOB)), 10),
            Error::<Test>::InsufficientFreeFunds
        );

        assert_eq!(events(), []);
    });
}

#[test]
fn publish_storage_deals_fails_empty_deals() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Market::publish_storage_deals(
                RuntimeOrigin::signed(account::<Test>(PROVIDER)),
                bounded_vec![]
            ),
            Error::<Test>::NoProposalsToBePublished
        );
    });
}

#[test]
fn publish_storage_deals_fails_caller_not_provider() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Market::publish_storage_deals(
                RuntimeOrigin::signed(account::<Test>(ALICE)),
                bounded_vec![DealProposalBuilder::<Test>::default().signed(ALICE)]
            ),
            Error::<Test>::ProposalsNotPublishedByStorageProvider
        );
    });
}

#[test]
fn publish_storage_deals_fails_invalid_signature() {
    new_test_ext().execute_with(|| {
        let mut deal = DealProposalBuilder::<Test>::default().signed(ALICE);
        // Change the message contents so the signature does not match
        deal.proposal.piece_size = 1337;

        assert_noop!(
            Market::publish_storage_deals(
                RuntimeOrigin::signed(account::<Test>(PROVIDER)),
                bounded_vec![deal]
            ),
            Error::<Test>::AllProposalsInvalid
        );
    });
}

#[test]
fn publish_storage_deals_fails_end_before_start() {
    new_test_ext().execute_with(|| {
        let proposal = DealProposalBuilder::<Test>::default()
            // Make start_block > end_block
            .start_block(1337)
            .signed(ALICE);

        assert_noop!(
            Market::publish_storage_deals(
                RuntimeOrigin::signed(account::<Test>(PROVIDER)),
                bounded_vec![proposal]
            ),
            Error::<Test>::AllProposalsInvalid
        );
    });
}

#[test]
fn publish_storage_deals_fails_must_be_unpublished() {
    new_test_ext().execute_with(|| {
        let proposal = DealProposalBuilder::<Test>::default()
            .state(DealState::Active(ActiveDealState {
                sector_number: 0,
                sector_start_block: 0,
                last_updated_block: Some(10),
                slash_block: None,
            }))
            .signed(ALICE);

        assert_noop!(
            Market::publish_storage_deals(
                RuntimeOrigin::signed(account::<Test>(PROVIDER)),
                bounded_vec![proposal]
            ),
            Error::<Test>::AllProposalsInvalid
        );
    });
}

#[test]
fn publish_storage_deals_fails_min_duration_out_of_bounds() {
    new_test_ext().execute_with(|| {
        let proposal = DealProposalBuilder::<Test>::default()
            .start_block(10)
            .end_block(10 + <<Test as Config>::MinDealDuration as Get<u64>>::get() - 1)
            .signed(ALICE);

        assert_noop!(
            Market::publish_storage_deals(
                RuntimeOrigin::signed(account::<Test>(PROVIDER)),
                bounded_vec![proposal]
            ),
            Error::<Test>::AllProposalsInvalid
        );
    });
}

#[test]
fn publish_storage_deals_fails_max_duration_out_of_bounds() {
    new_test_ext().execute_with(|| {
        let proposal = DealProposalBuilder::<Test>::default()
            .start_block(100)
            .end_block(100 + <<Test as Config>::MaxDealDuration as Get<u64>>::get() + 1)
            .signed(ALICE);

        assert_noop!(
            Market::publish_storage_deals(
                RuntimeOrigin::signed(account::<Test>(PROVIDER)),
                bounded_vec![proposal]
            ),
            Error::<Test>::AllProposalsInvalid
        );
    });
}

/// Add enough balance to the provider so that the first proposal can be accepted and published.
/// Second proposal will be rejected, but first still published
#[test]
fn publish_storage_deals_fails_different_providers() {
    new_test_ext().execute_with(|| {
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(PROVIDER)), 60);
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(ALICE)), 60);
        System::reset_events();

        assert_ok!(Market::publish_storage_deals(
            RuntimeOrigin::signed(account::<Test>(PROVIDER)),
            bounded_vec![
                DealProposalBuilder::<Test>::default().signed(ALICE),
                // Proposal where second deal's provider is not a caller
                DealProposalBuilder::<Test>::default()
                    .client(BOB)
                    .provider(BOB)
                    .signed(BOB),
            ]
        ));
        assert_eq!(
            events(),
            [RuntimeEvent::Market(Event::<Test>::DealPublished {
                deal_id: 0,
                client: account::<Test>(ALICE),
                provider: account::<Test>(PROVIDER),
            })]
        );
    });
}

/// Add enough balance to the provider so that the first proposal can be accepted and published.
/// Second proposal will be rejected, but first still published
#[test]
fn publish_storage_deals_fails_client_not_enough_funds_for_second_deal() {
    new_test_ext().execute_with(|| {
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(PROVIDER)), 60);
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(ALICE)), 60);
        System::reset_events();

        assert_ok!(Market::publish_storage_deals(
            RuntimeOrigin::signed(account::<Test>(PROVIDER)),
            bounded_vec![
                DealProposalBuilder::<Test>::default().signed(ALICE),
                DealProposalBuilder::<Test>::default()
                    .piece_size(10)
                    .signed(ALICE),
            ]
        ));
        assert_eq!(
            events(),
            [RuntimeEvent::Market(Event::<Test>::DealPublished {
                deal_id: 0,
                client: account::<Test>(ALICE),
                provider: account::<Test>(PROVIDER),
            })]
        );
    });
}

/// Add enough balance to the provider so that the first proposal can be accepted and published.
/// Collateral is 25 for the default deal, so provider should have at least 50.
/// Second proposal will be rejected, but first still published
#[test]
fn publish_storage_deals_fails_provider_not_enough_funds_for_second_deal() {
    new_test_ext().execute_with(|| {
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(PROVIDER)), 40);
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(ALICE)), 90);
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(BOB)), 90);
        System::reset_events();

        assert_ok!(Market::publish_storage_deals(
            RuntimeOrigin::signed(account::<Test>(PROVIDER)),
            bounded_vec![
                DealProposalBuilder::<Test>::default().signed(ALICE),
                DealProposalBuilder::<Test>::default()
                    .client(BOB)
                    .signed(BOB),
            ]
        ));
        assert_eq!(
            events(),
            [RuntimeEvent::Market(Event::<Test>::DealPublished {
                deal_id: 0,
                client: account::<Test>(ALICE),
                provider: account::<Test>(PROVIDER),
            })]
        );
    });
}

#[test]
fn publish_storage_deals_fails_duplicate_deal_in_message() {
    new_test_ext().execute_with(|| {
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(PROVIDER)), 90);
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(ALICE)), 90);
        System::reset_events();

        assert_ok!(Market::publish_storage_deals(
            RuntimeOrigin::signed(account::<Test>(PROVIDER)),
            bounded_vec![
                DealProposalBuilder::<Test>::default()
                    .storage_price_per_block(1)
                    .signed(ALICE),
                DealProposalBuilder::<Test>::default()
                    .storage_price_per_block(1)
                    .signed(ALICE),
            ]
        ));
        assert_eq!(
            events(),
            [RuntimeEvent::Market(Event::<Test>::DealPublished {
                deal_id: 0,
                client: account::<Test>(ALICE),
                provider: account::<Test>(PROVIDER),
            })]
        );
    });
}

#[test]
fn publish_storage_deals_fails_duplicate_deal_in_state() {
    new_test_ext().execute_with(|| {
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(PROVIDER)), 90);
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(ALICE)), 90);
        System::reset_events();

        assert_ok!(Market::publish_storage_deals(
            RuntimeOrigin::signed(account::<Test>(PROVIDER)),
            bounded_vec![DealProposalBuilder::<Test>::default()
                .storage_price_per_block(1)
                .signed(ALICE),]
        ));
        assert_eq!(
            events(),
            [RuntimeEvent::Market(Event::<Test>::DealPublished {
                deal_id: 0,
                client: account::<Test>(ALICE),
                provider: account::<Test>(PROVIDER),
            })]
        );
        assert_noop!(
            Market::publish_storage_deals(
                RuntimeOrigin::signed(account::<Test>(PROVIDER)),
                bounded_vec![DealProposalBuilder::<Test>::default()
                    .storage_price_per_block(1)
                    .signed(ALICE),]
            ),
            Error::<Test>::AllProposalsInvalid
        );
    });
}

#[test]
fn publish_storage_deals() {
    new_test_ext().execute_with(|| {
        let alice_proposal = DealProposalBuilder::<Test>::default().signed(ALICE);
        let alice_start_block = 100;
        let alice_deal_id = 0;
        // We're not expecting for it to go through, but the call should not fail.
        let alice_second_proposal = DealProposalBuilder::<Test>::default()
            .piece_size(37)
            .signed(ALICE);
        let bob_deal_id = 1;
        let bob_start_block = 130;
        let bob_proposal = DealProposalBuilder::<Test>::default()
            .client(BOB)
            .start_block(bob_start_block)
            .end_block(135)
            .storage_price_per_block(10)
            .provider_collateral(15)
            .signed(BOB);

        let alice_hash = Market::hash_proposal(&alice_proposal.proposal);
        let bob_hash = Market::hash_proposal(&bob_proposal.proposal);

        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(ALICE)), 60);
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(BOB)), 70);
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(PROVIDER)), 75);
        System::reset_events();

        assert_ok!(Market::publish_storage_deals(
            RuntimeOrigin::signed(account::<Test>(PROVIDER)),
            bounded_vec![alice_proposal, alice_second_proposal, bob_proposal]
        ));
        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(ALICE)),
            BalanceEntry::<u64> {
                free: 10,
                locked: 50
            }
        );
        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(BOB)),
            BalanceEntry::<u64> {
                free: 20,
                locked: 50
            }
        );
        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(PROVIDER)),
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
                    client: account::<Test>(ALICE),
                    provider: account::<Test>(PROVIDER),
                }),
                RuntimeEvent::Market(Event::<Test>::DealPublished {
                    deal_id: bob_deal_id,
                    client: account::<Test>(BOB),
                    provider: account::<Test>(PROVIDER),
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
    new_test_ext().execute_with(|| {
        publish_for_activation(1, DealProposalBuilder::<Test>::default().unsigned());

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
            Market::verify_deals_for_activation(&account::<Test>(PROVIDER), deals)
        );
    });
}

#[test]
fn verify_deals_for_activation_fails_with_different_provider() {
    new_test_ext().execute_with(|| {
        publish_for_activation(
            1,
            DealProposalBuilder::<Test>::default()
                .provider(BOB)
                .unsigned(),
        );

        let deals = bounded_vec![SectorDealBuilder::default().build()];

        assert_noop!(
            Market::verify_deals_for_activation(&account::<Test>(PROVIDER), deals),
            Error::<Test>::DealActivationError
        );
    });
}

#[test]
fn verify_deals_for_activation_fails_with_invalid_deal_state() {
    new_test_ext().execute_with(|| {
        publish_for_activation(
            1,
            DealProposalBuilder::<Test>::default()
                .state(DealState::Active(ActiveDealState {
                    sector_number: 0,
                    sector_start_block: 0,
                    last_updated_block: Some(10),
                    slash_block: None,
                }))
                .unsigned(),
        );

        let deals = bounded_vec![SectorDealBuilder::default().build()];

        assert_noop!(
            Market::verify_deals_for_activation(&account::<Test>(PROVIDER), deals),
            Error::<Test>::DealActivationError
        );
    });
}

#[test]
fn verify_deals_for_activation_fails_deal_not_in_pending() {
    new_test_ext().execute_with(|| {
        // do not use `publish_for_activation` as it puts deal in PendingProposals
        Proposals::<Test>::insert(1, DealProposalBuilder::<Test>::default().unsigned());
        let deals = bounded_vec![SectorDealBuilder::default().build()];

        assert_noop!(
            Market::verify_deals_for_activation(&account::<Test>(PROVIDER), deals),
            Error::<Test>::DealActivationError
        );
    });
}

#[test]
fn verify_deals_for_activation_fails_sector_activation_on_deal_from_the_past() {
    new_test_ext().execute_with(|| {
        // current_block == sector_activation when calling `verify_deals_for_activation`
        // wait a couple of blocks so deal cannot be activated, because it's too late.
        run_to_block(2);

        publish_for_activation(
            1,
            DealProposalBuilder::<Test>::default()
                .start_block(1)
                .unsigned(),
        );

        let deals = bounded_vec![SectorDealBuilder::default().build()];

        assert_noop!(
            Market::verify_deals_for_activation(&account::<Test>(PROVIDER), deals),
            Error::<Test>::DealActivationError
        );
    });
}

#[test]
fn verify_deals_for_activation_fails_sector_expires_before_deal_ends() {
    new_test_ext().execute_with(|| {
        publish_for_activation(
            1,
            DealProposalBuilder::<Test>::default()
                .start_block(10)
                .end_block(15)
                .unsigned(),
        );

        let deals = bounded_vec![SectorDealBuilder::default().sector_expiry(11).build()];

        assert_noop!(
            Market::verify_deals_for_activation(&account::<Test>(PROVIDER), deals),
            Error::<Test>::DealActivationError
        );
    });
}

#[test]
fn verify_deals_for_activation_fails_not_enough_space() {
    new_test_ext().execute_with(|| {
        publish_for_activation(
            1,
            DealProposalBuilder::<Test>::default()
                .piece_size(1 << 10 /* 1 KiB */)
                .unsigned(),
        );
        publish_for_activation(
            2,
            DealProposalBuilder::<Test>::default()
                .piece_size(3 << 10 /* 3 KiB */)
                .unsigned(),
        );
        // 1 KiB + 3KiB >= 2 KiB (sector size)

        let deals = bounded_vec![SectorDealBuilder::default()
            .deal_ids(bounded_vec![1, 2])
            .build()];

        assert_noop!(
            Market::verify_deals_for_activation(&account::<Test>(PROVIDER), deals),
            Error::<Test>::DealsTooLargeToFitIntoSector
        );
    });
}

#[test]
fn verify_deals_for_activation_fails_duplicate_deals() {
    new_test_ext().execute_with(|| {
        publish_for_activation(1, DealProposalBuilder::<Test>::default().unsigned());

        let deals = bounded_vec![SectorDealBuilder::default()
            .deal_ids(bounded_vec![1, 1])
            .build()];

        assert_noop!(
            Market::verify_deals_for_activation(&account::<Test>(PROVIDER), deals),
            Error::<Test>::DuplicateDeal
        );
    });
}

#[test]
fn verify_deals_for_activation_fails_deal_not_found() {
    new_test_ext().execute_with(|| {
        let deals = bounded_vec![SectorDealBuilder::default()
            .deal_ids(bounded_vec![1, 2, 3, 4])
            .build()];

        assert_noop!(
            Market::verify_deals_for_activation(&account::<Test>(PROVIDER), deals),
            Error::<Test>::DealNotFound
        );
    });
}

#[test]
fn activate_deals() {
    new_test_ext().execute_with(|| {
        let alice_hash =
            publish_for_activation(1, DealProposalBuilder::<Test>::default().unsigned());

        let deals = bounded_vec![
            SectorDealBuilder::default().build(),
            SectorDealBuilder::default()
                .sector_number(2)
                .sector_expiry(50)
                .deal_ids(bounded_vec![])
                .build()
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
                        client: account::<Test>(ALICE),
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
            Market::activate_deals(&account::<Test>(PROVIDER), deals, true)
        );
        assert!(!PendingProposals::<Test>::get().contains(&alice_hash));
    });
}

#[test]
fn activate_deals_fails_for_1_sector_but_succeeds_for_others() {
    new_test_ext().execute_with(|| {
        let alice_hash =
            publish_for_activation(1, DealProposalBuilder::<Test>::default().unsigned());
        let _ = publish_for_activation(2, DealProposalBuilder::<Test>::default().unsigned());
        let deals = bounded_vec![
            SectorDealBuilder::default().build(),
            SectorDealBuilder::default()
                .sector_number(2)
                .sector_expiry(50)
                .deal_ids(bounded_vec![])
                .build(),
            SectorDealBuilder::default()
                .sector_number(3)
                .deal_ids(bounded_vec![1337])
                .build(),
            SectorDealBuilder::default()
                .sector_number(4)
                // force error by making expiry < start_block
                .sector_expiry(10)
                .deal_ids(bounded_vec![2])
                .build()
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
                        client: account::<Test>(ALICE),
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
            Market::activate_deals(&account::<Test>(PROVIDER), deals, true)
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
    new_test_ext().execute_with(|| {
        let alice_start_block = 100;
        let alice_deal_id = 0;
        let alice_proposal = DealProposalBuilder::<Test>::default()
            .start_block(alice_start_block)
            .end_block(alice_start_block + 10)
            .storage_price_per_block(5)
            .provider_collateral(25)
            .signed(ALICE);

        let bob_start_block = 130;
        let bob_deal_id = 1;
        let bob_proposal = DealProposalBuilder::<Test>::default()
            .client(BOB)
            .start_block(bob_start_block)
            .end_block(bob_start_block + 5)
            .storage_price_per_block(10)
            .provider_collateral(15)
            .signed(BOB);

        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(ALICE)), 60);
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(BOB)), 70);
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(PROVIDER)), 75);
        let _ = Market::publish_storage_deals(
            RuntimeOrigin::signed(account::<Test>(PROVIDER)),
            bounded_vec![alice_proposal, bob_proposal],
        );
        let _ = Market::activate_deals(
            &account::<Test>(PROVIDER),
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
            BalanceTable::<Test>::get(account::<Test>(ALICE)),
            BalanceEntry::<u64> {
                free: 10,
                locked: 50
            }
        );
        // After Alice's block, nothing changes to the balance. It has been activated properly.
        run_to_block(alice_start_block + 1);
        assert!(!DealsForBlock::<Test>::get(&alice_start_block).contains(&alice_deal_id));
        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(ALICE)),
            BalanceEntry::<u64> {
                free: 10,
                locked: 50
            }
        );

        // Balances before processing the hook
        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(BOB)),
            BalanceEntry::<u64> {
                free: 20,
                locked: 50
            }
        );
        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(PROVIDER)),
            BalanceEntry::<u64> {
                free: 35,
                locked: 40
            }
        );
        // After exceeding Bob's deal start_block,
        // Storage Provider should be slashed for Bob's amount and Bob refunded.
        run_to_block(bob_start_block + 1);
        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(BOB)),
            BalanceEntry::<u64> {
                free: 70,
                locked: 0
            }
        );
        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(PROVIDER)),
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
    new_test_ext().execute_with(|| {
        assert_ok!(Market::settle_deal_payments(
            RuntimeOrigin::signed(account::<Test>(ALICE)),
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
    new_test_ext().execute_with(|| {
        let alice_proposal = DealProposalBuilder::<Test>::default().signed(ALICE);

        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(ALICE)), 60);
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(PROVIDER)), 75);

        assert_ok!(Market::publish_storage_deals(
            RuntimeOrigin::signed(account::<Test>(PROVIDER)),
            bounded_vec![alice_proposal]
        ));
        System::reset_events();

        assert_ok!(Market::settle_deal_payments(
            RuntimeOrigin::signed(account::<Test>(ALICE)),
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
    new_test_ext().execute_with(|| {
        let alice_proposal = DealProposalBuilder::<Test>::default()
            .start_block(0)
            .end_block(10)
            .signed(ALICE);

        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(ALICE)), 60);
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(BOB)), 70);
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(PROVIDER)), 75);

        assert_ok!(Market::publish_storage_deals(
            RuntimeOrigin::signed(account::<Test>(PROVIDER)),
            bounded_vec![alice_proposal]
        ));

        Proposals::<Test>::insert(
            1,
            DealProposalBuilder::<Test>::default()
                .client(BOB)
                .start_block(0)
                .end_block(10)
                .storage_price_per_block(10)
                .provider_collateral(15)
                .unsigned(),
        );

        System::reset_events();

        assert_ok!(Market::settle_deal_payments(
            RuntimeOrigin::signed(account::<Test>(ALICE)),
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
    new_test_ext().execute_with(|| {
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(ALICE)), 60);
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(PROVIDER)), 75);

        Proposals::<Test>::insert(
            0,
            DealProposalBuilder::<Test>::default()
                .start_block(0)
                .end_block(10)
                .state(DealState::Active(ActiveDealState {
                    sector_number: 0,
                    sector_start_block: 0,
                    last_updated_block: Some(10),
                    slash_block: None,
                }))
                .unsigned(),
        );
        System::reset_events();

        assert_ok!(Market::settle_deal_payments(
            RuntimeOrigin::signed(account::<Test>(ALICE)),
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
    new_test_ext().execute_with(|| {
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(ALICE)), 60);
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(PROVIDER)), 75);

        Proposals::<Test>::insert(
            0,
            DealProposalBuilder::<Test>::default()
                .start_block(0)
                .end_block(10)
                .state(DealState::Active(ActiveDealState {
                    sector_number: 0,
                    sector_start_block: 0,
                    last_updated_block: Some(11),
                    slash_block: None,
                }))
                .unsigned(),
        );
        run_to_block(12);
        System::reset_events();

        assert_err!(
            Market::settle_deal_payments(
                RuntimeOrigin::signed(account::<Test>(ALICE)),
                bounded_vec!(0)
            ),
            DispatchError::Corruption
        );

        assert_eq!(events(), [])
    });
}

#[test]
fn settle_deal_payments_success() {
    new_test_ext().execute_with(|| {
        let alice_proposal = DealProposalBuilder::<Test>::default()
            .start_block(0)
            .end_block(10)
            .signed(ALICE);

        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(ALICE)), 60);
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(PROVIDER)), 75);

        assert_ok!(Market::publish_storage_deals(
            RuntimeOrigin::signed(account::<Test>(PROVIDER)),
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
            Some(
                DealProposalBuilder::<Test>::default()
                    .start_block(0)
                    .end_block(10)
                    .state(DealState::Active(ActiveDealState {
                        sector_number: 0,
                        sector_start_block: 0,
                        last_updated_block: None,
                        slash_block: None,
                    }))
                    .unsigned()
            )
        );
        System::reset_events();

        run_to_block(5);

        assert_ok!(Market::settle_deal_payments(
            RuntimeOrigin::signed(account::<Test>(ALICE)),
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
            BalanceTable::<Test>::get(account::<Test>(PROVIDER)),
            BalanceEntry::<u64> {
                free: 75, // 50 (from 75 - collateral) + 5 * 5 (price per block * n blocks)
                locked: 25
            }
        );

        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(ALICE)),
            BalanceEntry::<u64> {
                free: 10,
                locked: 25, // 50 - 5 * 5 (price per block * n blocks)
            }
        );

        assert_eq!(
            Proposals::<Test>::get(0),
            Some(
                DealProposalBuilder::<Test>::default()
                    .start_block(0)
                    .end_block(10)
                    .state(DealState::Active(ActiveDealState {
                        sector_number: 0,
                        sector_start_block: 0,
                        last_updated_block: Some(5),
                        slash_block: None,
                    }))
                    .unsigned()
            )
        );
    });
}

#[test]
fn settle_deal_payments_success_finished() {
    new_test_ext().execute_with(|| {
        let alice_proposal = DealProposalBuilder::<Test>::default()
            .start_block(0)
            .end_block(10)
            .signed(ALICE);

        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(ALICE)), 60);
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(PROVIDER)), 75);

        assert_ok!(Market::publish_storage_deals(
            RuntimeOrigin::signed(account::<Test>(PROVIDER)),
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
            Some(
                DealProposalBuilder::<Test>::default()
                    .start_block(0)
                    .end_block(10)
                    .state(DealState::Active(ActiveDealState {
                        sector_number: 0,
                        sector_start_block: 0,
                        last_updated_block: None,
                        slash_block: None,
                    }))
                    .unsigned()
            )
        );

        System::reset_events();

        // Deal is finished
        run_to_block(11);

        assert_ok!(Market::settle_deal_payments(
            RuntimeOrigin::signed(account::<Test>(ALICE)),
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
            BalanceTable::<Test>::get(account::<Test>(PROVIDER)),
            BalanceEntry::<u64> {
                free: 50 + 5 * 10, // 50 (from 75 - collateral) + (price per block * n blocks)
                locked: 25
            }
        );

        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(ALICE)),
            BalanceEntry::<u64> {
                free: 10,
                locked: 50 - 5 * 10, // locked - (price per block * n blocks)
            }
        );

        assert_eq!(Proposals::<Test>::get(0), None);
    });
}

#[test]
fn test_lock_funds() {
    let _ = env_logger::try_init();
    new_test_ext().execute_with(|| {
        assert_eq!(
            <Test as crate::pallet::Config>::Currency::total_balance(&account::<Test>(PROVIDER)),
            100
        );
        // We can't get all 100, otherwise the account would be reaped
        assert_ok!(Market::add_balance(
            RuntimeOrigin::signed(account::<Test>(PROVIDER)),
            90
        ));
        assert_eq!(
            <Test as crate::pallet::Config>::Currency::total_balance(&account::<Test>(PROVIDER)),
            10
        );
        assert_ok!(lock_funds::<Test>(&account::<Test>(PROVIDER), 25));
        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(PROVIDER)),
            BalanceEntry::<u64> {
                free: 65,
                locked: 25,
            }
        );

        assert_ok!(lock_funds::<Test>(&account::<Test>(PROVIDER), 65));
        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(PROVIDER)),
            BalanceEntry::<u64> {
                free: 0,
                locked: 90,
            }
        );

        assert_err!(
            lock_funds::<Test>(&account::<Test>(PROVIDER), 25),
            DispatchError::Arithmetic(ArithmeticError::Underflow)
        );

        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(PROVIDER)),
            BalanceEntry::<u64> {
                free: 0,
                locked: 90,
            }
        );
    });
}

#[test]
fn test_unlock_funds() {
    let _ = env_logger::try_init();
    new_test_ext().execute_with(|| {
        assert_eq!(
            <Test as crate::pallet::Config>::Currency::total_balance(&account::<Test>(PROVIDER)),
            100
        );
        // We can't get all 100, otherwise the account would be reaped
        assert_ok!(Market::add_balance(
            RuntimeOrigin::signed(account::<Test>(PROVIDER)),
            90
        ));
        assert_eq!(
            <Test as crate::pallet::Config>::Currency::total_balance(&account::<Test>(PROVIDER)),
            10
        );
        assert_ok!(lock_funds::<Test>(&account::<Test>(PROVIDER), 90));
        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(PROVIDER)),
            BalanceEntry::<u64> {
                free: 0,
                locked: 90,
            }
        );

        assert_ok!(unlock_funds::<Test>(&account::<Test>(PROVIDER), 30));
        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(PROVIDER)),
            BalanceEntry::<u64> {
                free: 30,
                locked: 60,
            }
        );

        assert_ok!(unlock_funds::<Test>(&account::<Test>(PROVIDER), 60));
        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(PROVIDER)),
            BalanceEntry::<u64> {
                free: 90,
                locked: 0,
            }
        );

        assert_err!(
            unlock_funds::<Test>(&account::<Test>(PROVIDER), 60),
            DispatchError::Arithmetic(ArithmeticError::Underflow)
        );
        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(PROVIDER)),
            BalanceEntry::<u64> {
                free: 90,
                locked: 0,
            }
        );
    });
}

#[test]
fn slash_and_burn_acc() {
    let _ = env_logger::try_init();
    new_test_ext().execute_with(|| {
        assert_eq!(
            <Test as crate::pallet::Config>::Currency::total_issuance(),
            300
        );
        assert_ok!(Market::add_balance(
            RuntimeOrigin::signed(account::<Test>(PROVIDER)),
            75
        ));

        System::reset_events();

        assert_ok!(lock_funds::<Test>(&account::<Test>(PROVIDER), 10));
        assert_ok!(slash_and_burn::<Test>(&account::<Test>(PROVIDER), 10));

        assert_eq!(
            events(),
            [RuntimeEvent::Balances(
                pallet_balances::Event::<Test>::Withdraw {
                    who: Market::account_id(),
                    amount: 10
                }
            ),]
        );
        assert_eq!(
            <Test as crate::pallet::Config>::Currency::total_issuance(),
            290
        );

        assert_eq!(
            BalanceTable::<Test>::get(account::<Test>(PROVIDER)),
            BalanceEntry::<u64> {
                free: 65,
                locked: 0,
            }
        );

        assert_err!(
            slash_and_burn::<Test>(&account::<Test>(PROVIDER), 10),
            DispatchError::Arithmetic(ArithmeticError::Underflow)
        );
        assert_eq!(
            <Test as crate::pallet::Config>::Currency::total_issuance(),
            290
        );
    });
}

#[test]
fn on_sector_terminate_unknown_deals() {
    let _ = env_logger::try_init();
    new_test_ext().execute_with(|| {
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(PROVIDER)), 75);
        System::reset_events();

        let cid = BoundedVec::try_from(cid_of("polka_storage_cid").to_bytes()).unwrap();
        assert_ok!(Market::on_sectors_terminate(
            &account::<Test>(PROVIDER),
            bounded_vec![cid],
        ));

        assert_eq!(events(), []);
    });
}

#[test]
fn on_sector_terminate_deal_not_found() {
    let _ = env_logger::try_init();
    new_test_ext().execute_with(|| {
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(PROVIDER)), 75);
        System::reset_events();

        let cid = BoundedVec::try_from(cid_of("polka_storage_cid").to_bytes()).unwrap();
        let sector_deal_ids: BoundedVec<_, ConstU32<MAX_DEALS_PER_SECTOR>> = bounded_vec![1];

        SectorDeals::<Test>::insert(cid.clone(), sector_deal_ids);

        assert_err!(
            Market::on_sectors_terminate(&account::<Test>(PROVIDER), bounded_vec![cid]),
            DispatchError::from(SectorTerminateError::DealNotFound)
        );

        assert_eq!(events(), []);
    });
}

#[test]
fn on_sector_terminate_invalid_caller() {
    let _ = env_logger::try_init();
    new_test_ext().execute_with(|| {
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(PROVIDER)), 75);
        System::reset_events();

        let cid = BoundedVec::try_from(cid_of("polka_storage_cid").to_bytes()).unwrap();
        let sector_deal_ids: BoundedVec<_, ConstU32<MAX_DEALS_PER_SECTOR>> = bounded_vec![1];

        SectorDeals::<Test>::insert(cid.clone(), sector_deal_ids);
        Proposals::<Test>::insert(
            1,
            DealProposalBuilder::<Test>::default()
                .client(BOB)
                .unsigned(),
        );

        assert_err!(
            Market::on_sectors_terminate(&account::<Test>(BOB), bounded_vec![cid],),
            DispatchError::from(SectorTerminateError::InvalidCaller)
        );

        assert_eq!(events(), []);
    });
}

#[test]
fn on_sector_terminate_not_active() {
    let _ = env_logger::try_init();
    new_test_ext().execute_with(|| {
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(PROVIDER)), 75);
        System::reset_events();

        let cid = BoundedVec::try_from(cid_of("polka_storage_cid").to_bytes()).unwrap();
        let sector_deal_ids: BoundedVec<_, ConstU32<MAX_DEALS_PER_SECTOR>> = bounded_vec![1];

        SectorDeals::<Test>::insert(cid.clone(), sector_deal_ids);
        Proposals::<Test>::insert(
            1,
            DealProposalBuilder::<Test>::default()
                .client(BOB)
                .start_block(0)
                .end_block(10)
                .storage_price_per_block(10)
                .provider_collateral(15)
                .unsigned(),
        );

        assert_err!(
            Market::on_sectors_terminate(&account::<Test>(PROVIDER), bounded_vec![cid],),
            DispatchError::from(SectorTerminateError::DealIsNotActive)
        );

        assert_eq!(events(), []);
    });
}

#[test]
fn on_sector_terminate_active() {
    let _ = env_logger::try_init();
    new_test_ext().execute_with(|| {
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(BOB)), 75);
        let _ = Market::add_balance(RuntimeOrigin::signed(account::<Test>(PROVIDER)), 75);

        let cid = BoundedVec::try_from(cid_of("polka_storage_cid").to_bytes()).unwrap();
        let sector_deal_ids: BoundedVec<_, ConstU32<MAX_DEALS_PER_SECTOR>> = bounded_vec![1];
        let deal_proposal = DealProposalBuilder::<Test>::default()
            .client(BOB)
            .start_block(0)
            .end_block(10)
            .storage_price_per_block(5)
            .provider_collateral(15)
            .state(DealState::Active(ActiveDealState::new(0, 0)))
            .unsigned();

        assert_ok!(lock_funds::<Test>(&account::<Test>(BOB), 5 * 10));
        assert_ok!(lock_funds::<Test>(&account::<Test>(PROVIDER), 15));

        let hash_proposal = Market::hash_proposal(&deal_proposal);
        let mut pending = PendingProposals::<Test>::get();
        pending
            .try_insert(hash_proposal)
            .expect("should have enough space");
        PendingProposals::<Test>::set(pending);

        SectorDeals::<Test>::insert(cid.clone(), sector_deal_ids);
        Proposals::<Test>::insert(1, deal_proposal);

        System::reset_events();

        assert_ok!(Market::on_sectors_terminate(
            &account::<Test>(PROVIDER),
            bounded_vec![cid],
        ));

        assert_eq!(
            BalanceTable::<Test>::get(&account::<Test>(BOB)),
            BalanceEntry {
                free: 70,  // unlocked funds - 5 for the storage payment of a single block
                locked: 0, // unlocked
            }
        );

        assert_eq!(
            BalanceTable::<Test>::get(&account::<Test>(PROVIDER)),
            BalanceEntry {
                free: 65,  // the original 60 + 5 for the storage payment of a single block
                locked: 0, // lost the 15 collateral
            }
        );

        assert_eq!(
            events(),
            [
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Withdraw {
                    who: Market::account_id(),
                    amount: 15
                }),
                RuntimeEvent::Market(Event::<Test>::DealTerminated {
                    deal_id: 1,
                    client: account::<Test>(BOB),
                    provider: account::<Test>(PROVIDER)
                })
            ]
        );
        assert!(PendingProposals::<Test>::get().is_empty());
        assert!(!Proposals::<Test>::contains_key(1));
        assert_eq!(
            <Test as crate::pallet::Config>::Currency::total_issuance(),
            285
        );
    });
}

/// Builder with nice defaults for test purposes.
struct SectorDealBuilder {
    sector_number: u64,
    sector_expiry: u64,
    sector_type: RegisteredSealProof,
    deal_ids: BoundedVec<DealId, ConstU32<MAX_DEALS_PER_SECTOR>>,
}

impl SectorDealBuilder {
    pub fn sector_expiry(mut self, sector_expiry: u64) -> Self {
        self.sector_expiry = sector_expiry;
        self
    }

    pub fn sector_number(mut self, sector_number: u64) -> Self {
        self.sector_number = sector_number;
        self
    }

    pub fn deal_ids(
        mut self,
        deal_ids: BoundedVec<DealId, ConstU32<MAX_DEALS_PER_SECTOR>>,
    ) -> Self {
        self.deal_ids = deal_ids;
        self
    }

    pub fn build(self) -> SectorDeal<u64> {
        SectorDeal::<u64> {
            sector_number: self.sector_number,
            sector_expiry: self.sector_expiry,
            sector_type: self.sector_type,
            deal_ids: self.deal_ids,
        }
    }
}

impl Default for SectorDealBuilder {
    fn default() -> Self {
        Self {
            sector_number: 1,
            sector_expiry: 120,
            sector_type: RegisteredSealProof::StackedDRG2KiBV1P1,
            deal_ids: bounded_vec![1],
        }
    }
}

/// Builder to simplify writing complex tests of [`DealProposal`].
/// Exclusively uses [`Test`] for simplification purposes.
pub struct DealProposalBuilder<T: frame_system::Config> {
    piece_cid: BoundedVec<u8, ConstU32<128>>,
    piece_size: u64,
    client: AccountIdOf<T>,
    provider: AccountIdOf<T>,
    label: BoundedVec<u8, ConstU32<128>>,
    start_block: u64,
    end_block: u64,
    storage_price_per_block: u64,
    provider_collateral: u64,
    state: DealState<u64>,
}

impl<T: frame_system::Config<AccountId = AccountId32>> Default for DealProposalBuilder<T> {
    fn default() -> Self {
        Self {
            piece_cid: cid_of("polka-storage-data")
                .to_bytes()
                .try_into()
                .expect("hash is always 32 bytes"),
            piece_size: 18,
            client: account::<Test>(ALICE),
            provider: account::<Test>(PROVIDER),
            label: bounded_vec![0xb, 0xe, 0xe, 0xf],
            start_block: 100,
            end_block: 110,
            storage_price_per_block: 5,
            provider_collateral: 25,
            // TODO(@th7nder,01/07/2024): change this to Published
            state: DealState::Published,
        }
    }
}

impl<T: frame_system::Config<AccountId = AccountId32>> DealProposalBuilder<T> {
    pub fn client(mut self, client: &'static str) -> Self {
        self.client = account::<Test>(client);
        self
    }

    pub fn provider(mut self, provider: &'static str) -> Self {
        self.provider = account::<Test>(provider);
        self
    }

    pub fn state(mut self, state: DealState<u64>) -> Self {
        self.state = state;
        self
    }

    pub fn start_block(mut self, start_block: u64) -> Self {
        self.start_block = start_block;
        self
    }

    pub fn end_block(mut self, end_block: u64) -> Self {
        self.end_block = end_block;
        self
    }

    pub fn storage_price_per_block(mut self, price: u64) -> Self {
        self.storage_price_per_block = price;
        self
    }

    pub fn provider_collateral(mut self, price: u64) -> Self {
        self.provider_collateral = price;
        self
    }

    pub fn piece_size(mut self, piece_size: u64) -> Self {
        self.piece_size = piece_size;
        self
    }

    pub fn unsigned(self) -> DealProposalOf<Test> {
        DealProposalOf::<Test> {
            piece_cid: self.piece_cid,
            piece_size: self.piece_size,
            client: self.client,
            provider: self.provider,
            label: self.label,
            start_block: self.start_block,
            end_block: self.end_block,
            storage_price_per_block: self.storage_price_per_block,
            provider_collateral: self.provider_collateral,
            state: self.state,
        }
    }

    pub fn signed(self, by: &'static str) -> ClientDealProposalOf<Test> {
        let built = self.unsigned();
        let signed = sign_proposal(by, built);
        signed
    }
}
