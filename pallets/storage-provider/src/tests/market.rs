use core::str::FromStr;

use cid::Cid;
use frame_support::{
    assert_err, assert_noop, assert_ok,
    pallet_prelude::{ConstU32, Get},
    sp_runtime::{bounded_vec, ArithmeticError, DispatchError, TokenError},
    traits::Currency,
    BoundedVec,
};
use primitives::{
    configs::CurrencyProvider,
    deals::{ActiveDealState, DealProposalOf, DealState},
    pallets::{ActiveDeal, ActiveSector, SectorDeal},
    proofs::RegisteredSealProof,
    DealId, MAX_DEALS_PER_SECTOR,
};
use sp_core::H256;

use crate::{
    balance::BalanceEntry,
    deal::{
        parameters::{OffchainDealDurationBound, OffchainDealParameters},
        DealSettlementError, PublishedDeal, SettledDealData,
    },
    lock_funds,
    pallet::{Error, Event},
    slash_and_burn,
    tests::{
        account, events, new_test_ext, register_storage_provider, run_to_block, Balances,
        DealProposalBuilder, RuntimeEvent, RuntimeOrigin, SectorDealBuilder, StorageProvider,
        System, Test, ALICE, BOB, CHARLIE, INITIAL_FUNDS,
    },
    unlock_funds, BalanceTable, Config, DealsForBlock, PendingProposals, Proposals,
    SPDealParameters, SectorDeals,
};

#[test]
fn initial_state() {
    new_test_ext().execute_with(|| {
        assert_eq!(Balances::free_balance(StorageProvider::account_id()), 0);
        assert_eq!(
            BalanceTable::<Test>::get(account(ALICE)),
            BalanceEntry::<u64> { free: 0, locked: 0 }
        );
    });
}

#[test]
fn adds_and_withdraws_balances() {
    new_test_ext().execute_with(|| {
        // Adds funds from an account to the pallet
        assert_ok!(StorageProvider::add_balance(
            RuntimeOrigin::signed(account(ALICE)),
            10
        ));
        assert_eq!(Balances::free_balance(StorageProvider::account_id()), 10);
        assert_eq!(Balances::free_balance(account(ALICE)), INITIAL_FUNDS - 10);
        assert_eq!(
            BalanceTable::<Test>::get(account(ALICE)),
            BalanceEntry::<u64> {
                free: 10,
                locked: 0,
            }
        );

        // Is able to withdraw added funds back
        assert_ok!(StorageProvider::withdraw_balance(
            RuntimeOrigin::signed(account(ALICE)),
            10
        ));
        assert_eq!(Balances::free_balance(StorageProvider::account_id()), 0);
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
        assert_ok!(StorageProvider::add_balance(
            RuntimeOrigin::signed(account(ALICE)),
            10
        ));
        assert_eq!(Balances::free_balance(StorageProvider::account_id()), 10);
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
                    account: StorageProvider::account_id()
                }),
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Endowed {
                    account: StorageProvider::account_id(),
                    free_balance: 10
                }),
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Transfer {
                    from: account(ALICE),
                    to: StorageProvider::account_id(),
                    amount: 10
                }),
                RuntimeEvent::StorageProvider(Event::<Test>::BalanceAdded {
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
            StorageProvider::add_balance(RuntimeOrigin::signed(account(ALICE)), INITIAL_FUNDS + 1),
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
            StorageProvider::add_balance(RuntimeOrigin::signed(account(BOB)), 1),
            ArithmeticError::Overflow
        );
    });
}

#[test]
fn withdraws_balance() {
    new_test_ext().execute_with(|| {
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(ALICE)), 10);
        System::reset_events();

        assert_ok!(StorageProvider::withdraw_balance(
            RuntimeOrigin::signed(account(ALICE)),
            10
        ));
        assert_eq!(Balances::free_balance(StorageProvider::account_id()), 0);
        assert_eq!(Balances::free_balance(account(ALICE)), INITIAL_FUNDS);
        assert_eq!(
            BalanceTable::<Test>::get(account(ALICE)),
            BalanceEntry::<u64> { free: 0, locked: 0 }
        );

        assert_eq!(
            events(),
            [
                RuntimeEvent::System(frame_system::Event::<Test>::KilledAccount {
                    account: StorageProvider::account_id()
                }),
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Transfer {
                    from: StorageProvider::account_id(),
                    to: account(ALICE),
                    amount: 10
                }),
                RuntimeEvent::StorageProvider(Event::<Test>::BalanceWithdrawn {
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
            StorageProvider::withdraw_balance(RuntimeOrigin::signed(account(BOB)), 10),
            Error::<Test>::InsufficientFreeFunds
        );

        assert_eq!(events(), []);
    });
}

/// NOTE: Success case should be tested but cannot be isolated.
/// Deal functionality with a registered storage provider is tested in success cases.
#[test]
fn publish_storage_deals_fails_sp_not_registered() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            StorageProvider::publish_storage_deals(
                RuntimeOrigin::signed(account(CHARLIE)),
                bounded_vec![DealProposalBuilder::default()
                    .client(ALICE)
                    .provider(&CHARLIE)
                    .signed(ALICE)]
            ),
            Error::<Test>::StorageProviderNotRegistered
        );
    });
}

#[test]
fn publish_storage_deals_fails_empty_deals() {
    new_test_ext().execute_with(|| {
        register_storage_provider(account(CHARLIE));
        assert_noop!(
            StorageProvider::publish_storage_deals(
                RuntimeOrigin::signed(account(CHARLIE)),
                bounded_vec![]
            ),
            Error::<Test>::NoProposalsToBePublished
        );
    });
}

#[test]
fn publish_storage_deals_fails_caller_not_provider() {
    new_test_ext().execute_with(|| {
        register_storage_provider(account(ALICE));
        assert_noop!(
            StorageProvider::publish_storage_deals(
                RuntimeOrigin::signed(account(ALICE)),
                bounded_vec![DealProposalBuilder::default()
                    .client(ALICE)
                    .provider(&CHARLIE)
                    .signed(ALICE)]
            ),
            Error::<Test>::ProposalsPublishedByIncorrectStorageProvider
        );
    });
}

#[test]
fn publish_storage_deals_fails_invalid_signature() {
    new_test_ext().execute_with(|| {
        register_storage_provider(account(CHARLIE));
        let mut deal = DealProposalBuilder::default()
            .client(ALICE)
            .provider(&CHARLIE)
            .signed(ALICE);
        // Change the message contents so the signature does not match
        deal.proposal.piece_size = 1337;

        assert_noop!(
            StorageProvider::publish_storage_deals(
                RuntimeOrigin::signed(account(CHARLIE)),
                bounded_vec![deal]
            ),
            Error::<Test>::WrongClientSignatureOnProposal
        );
    });
}

#[test]
fn publish_storage_deals_fails_end_before_start() {
    new_test_ext().execute_with(|| {
        register_storage_provider(account(CHARLIE));
        let proposal = DealProposalBuilder::default()
            .client(ALICE)
            .provider(&CHARLIE)
            // Make start_block > end_block
            .start_block(1337)
            .signed(ALICE);

        assert_noop!(
            StorageProvider::publish_storage_deals(
                RuntimeOrigin::signed(account(CHARLIE)),
                bounded_vec![proposal]
            ),
            Error::<Test>::DealEndBeforeStart
        );
    });
}

#[test]
fn publish_storage_deals_fails_must_be_unpublished() {
    new_test_ext().execute_with(|| {
        register_storage_provider(account(CHARLIE));
        let proposal = DealProposalBuilder::default()
            .client(ALICE)
            .provider(&CHARLIE)
            .state(DealState::Active(ActiveDealState {
                sector_number: 0.into(),
                sector_start_block: 0,
                last_updated_block: Some(10),
                slash_block: None,
            }))
            .signed(ALICE);

        assert_noop!(
            StorageProvider::publish_storage_deals(
                RuntimeOrigin::signed(account(CHARLIE)),
                bounded_vec![proposal]
            ),
            Error::<Test>::DealNotPublished
        );
    });
}

#[test]
fn publish_storage_deals_fails_min_duration_out_of_bounds() {
    new_test_ext().execute_with(|| {
        register_storage_provider(account(CHARLIE));
        let proposal = DealProposalBuilder::default()
            .client(ALICE)
            .provider(&CHARLIE)
            .start_block(10)
            .end_block(10 + <<Test as Config>::MinDealDuration as Get<u64>>::get() - 1)
            .signed(ALICE);

        assert_noop!(
            StorageProvider::publish_storage_deals(
                RuntimeOrigin::signed(account(CHARLIE)),
                bounded_vec![proposal]
            ),
            Error::<Test>::DealDurationOutOfBounds
        );
    });
}

#[test]
fn publish_storage_deals_fails_max_duration_out_of_bounds() {
    new_test_ext().execute_with(|| {
        register_storage_provider(account(CHARLIE));
        let proposal = DealProposalBuilder::default()
            .client(ALICE)
            .provider(&CHARLIE)
            .start_block(100)
            .end_block(100 + <<Test as Config>::MaxDealDuration as Get<u64>>::get() + 1)
            .signed(ALICE);

        assert_noop!(
            StorageProvider::publish_storage_deals(
                RuntimeOrigin::signed(account(CHARLIE)),
                bounded_vec![proposal]
            ),
            Error::<Test>::DealDurationOutOfBounds
        );
    });
}

#[test]
fn publish_storage_deals_fails_start_time_expired() {
    new_test_ext().execute_with(|| {
        register_storage_provider(account(CHARLIE));
        run_to_block(101);

        let proposal = DealProposalBuilder::default()
            .client(ALICE)
            .provider(&CHARLIE)
            .start_block(100)
            .end_block(100 + <<Test as Config>::MaxDealDuration as Get<u64>>::get() + 1)
            .signed(ALICE);

        assert_noop!(
            StorageProvider::publish_storage_deals(
                RuntimeOrigin::signed(account(CHARLIE)),
                bounded_vec![proposal]
            ),
            Error::<Test>::DealStartExpired
        );
    });
}

/// Add enough balance to the provider so that the first proposal can be accepted and published.
/// All proposals will be rejected
#[test]
fn publish_storage_deals_fails_different_providers() {
    new_test_ext().execute_with(|| {
        register_storage_provider(account(CHARLIE));
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(CHARLIE)), 100);
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(ALICE)), 60);
        System::reset_events();

        assert_noop!(
            StorageProvider::publish_storage_deals(
                RuntimeOrigin::signed(account(CHARLIE)),
                bounded_vec![
                    DealProposalBuilder::default()
                        .client(ALICE)
                        .provider(&CHARLIE)
                        .signed(ALICE),
                    // Proposal where second deal's provider is not a caller
                    DealProposalBuilder::default()
                        .client(ALICE)
                        .provider(&CHARLIE)
                        .client(BOB)
                        .provider(BOB)
                        .signed(BOB),
                ]
            ),
            Error::<Test>::ProposalsPublishedByIncorrectStorageProvider
        );
        assert_eq!(events(), []);
    });
}

/// Add enough balance to the provider so that the first proposal can be accepted and published.
/// All proposals will be rejected
#[test]
fn publish_storage_deals_fails_client_not_enough_funds_for_second_deal() {
    new_test_ext().execute_with(|| {
        register_storage_provider(account(CHARLIE));
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(CHARLIE)), 100);
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(ALICE)), 60);
        System::reset_events();

        assert_noop!(
            StorageProvider::publish_storage_deals(
                RuntimeOrigin::signed(account(CHARLIE)),
                bounded_vec![
                    DealProposalBuilder::default()
                        .client(ALICE)
                        .provider(&CHARLIE)
                        .signed(ALICE),
                    DealProposalBuilder::default()
                        .client(ALICE)
                        .provider(&CHARLIE)
                        .piece_size(10)
                        .signed(ALICE),
                ]
            ),
            Error::<Test>::InsufficientFreeFunds
        );
        assert_eq!(events(), []);
    });
}

/// Add enough balance to the provider so that the first proposal can be accepted and published.
/// Collateral is 25 for the default deal, so provider should have at least 50.
/// Both proposals will be rejected
#[test]
fn publish_storage_deals_fails_provider_not_enough_funds_for_second_deal() {
    new_test_ext().execute_with(|| {
        register_storage_provider(account(CHARLIE));
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(CHARLIE)), 40);
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(ALICE)), 90);
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(BOB)), 90);
        System::reset_events();

        assert_noop!(
            StorageProvider::publish_storage_deals(
                RuntimeOrigin::signed(account(CHARLIE)),
                bounded_vec![
                    DealProposalBuilder::default()
                        .client(ALICE)
                        .provider(&CHARLIE)
                        .signed(ALICE),
                    DealProposalBuilder::default()
                        .client(ALICE)
                        .provider(&CHARLIE)
                        .client(BOB)
                        .signed(BOB),
                ]
            ),
            Error::<Test>::InsufficientFreeFunds
        );
        assert_eq!(events(), []);
    });
}

#[test]
fn publish_storage_deals_fails_duplicate_deal_in_message() {
    new_test_ext().execute_with(|| {
        register_storage_provider(account(CHARLIE));
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(CHARLIE)), 90);
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(ALICE)), 90);
        System::reset_events();

        assert_noop!(
            StorageProvider::publish_storage_deals(
                RuntimeOrigin::signed(account(CHARLIE)),
                bounded_vec![
                    DealProposalBuilder::default()
                        .client(ALICE)
                        .provider(&CHARLIE)
                        .storage_price_per_block(1)
                        .signed(ALICE),
                    DealProposalBuilder::default()
                        .client(ALICE)
                        .provider(&CHARLIE)
                        .storage_price_per_block(1)
                        .signed(ALICE),
                ]
            ),
            Error::<Test>::DuplicateDeal
        );
        assert_eq!(events(), []);
    });
}

#[test]
fn publish_storage_deals_fails_duplicate_deal_in_state() {
    new_test_ext().execute_with(|| {
        register_storage_provider(account(CHARLIE));
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(CHARLIE)), 90);
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(ALICE)), 90);
        System::reset_events();

        assert_ok!(StorageProvider::publish_storage_deals(
            RuntimeOrigin::signed(account(CHARLIE)),
            bounded_vec![DealProposalBuilder::default()
                .client(ALICE)
                .provider(&CHARLIE)
                .storage_price_per_block(1)
                .signed(ALICE),]
        ));
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(
                Event::<Test>::DealsPublished {
                    provider: account(CHARLIE),
                    deals: bounded_vec!(PublishedDeal {
                        deal_id: 0,
                        client: account(ALICE),
                    })
                }
            )]
        );
        assert_noop!(
            StorageProvider::publish_storage_deals(
                RuntimeOrigin::signed(account(CHARLIE)),
                bounded_vec![DealProposalBuilder::default()
                    .client(ALICE)
                    .provider(&CHARLIE)
                    .storage_price_per_block(1)
                    .signed(ALICE),]
            ),
            Error::<Test>::DuplicateDeal
        );
    });
}

#[test]
fn publish_storage_deals_fails_not_within_deal_parameters() {
    new_test_ext().execute_with(|| {
        register_storage_provider(account(CHARLIE));
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(CHARLIE)), 90);
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(ALICE)), 90);
        // Default price = 5, default duration = 10
        let deal_params: OffchainDealParameters<u64, u64> = OffchainDealParameters {
            minimum_price_per_block: 10,
            deal_duration: OffchainDealDurationBound {
                lower: Some(3),  // Chain minimum = 2
                upper: Some(29), // Chain maximum = 30
            },
        };
        assert_ok!(StorageProvider::publish_deal_parameters(
            RuntimeOrigin::signed(account(CHARLIE)),
            deal_params
        ));
        System::reset_events();

        // Fail on duration
        assert_noop!(
            StorageProvider::publish_storage_deals(
                RuntimeOrigin::signed(account(CHARLIE)),
                bounded_vec![DealProposalBuilder::default()
                    .client(ALICE)
                    .provider(&CHARLIE)
                    .signed(ALICE)]
            ),
            Error::<Test>::OutOfBoundsDeal
        );

        // Fail on price
        assert_noop!(
            StorageProvider::publish_storage_deals(
                RuntimeOrigin::signed(account(CHARLIE)),
                bounded_vec![DealProposalBuilder::default()
                    .client(ALICE)
                    .provider(&CHARLIE)
                    .end_block(107)
                    .signed(ALICE)]
            ),
            Error::<Test>::OutOfBoundsDeal
        );
    })
}

#[test]
fn publish_storage_deals() {
    new_test_ext().execute_with(|| {
        register_storage_provider(account(CHARLIE));
        let alice_proposal = DealProposalBuilder::default()
            .client(ALICE)
            .provider(&CHARLIE)
            .signed(ALICE);
        let alice_start_block = 100;
        let alice_deal_id = 0;
        let alice_second_deal_id = 1;
        // We're not expecting for it to go through, but the call should not fail.
        let alice_second_proposal = DealProposalBuilder::default()
            .client(ALICE)
            .provider(&CHARLIE)
            .piece_size(37)
            .signed(ALICE);
        let bob_deal_id = 2;
        let bob_start_block = 130;
        let bob_proposal = DealProposalBuilder::default()
            .client(ALICE)
            .provider(&CHARLIE)
            .client(BOB)
            .start_block(bob_start_block)
            .end_block(135)
            .storage_price_per_block(10)
            .signed(BOB);

        let alice_hash = StorageProvider::hash_proposal(&alice_proposal.proposal);
        let bob_hash = StorageProvider::hash_proposal(&bob_proposal.proposal);

        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(ALICE)), 100);
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(BOB)), 70);
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(CHARLIE)), 310);
        System::reset_events();

        assert_ok!(StorageProvider::publish_storage_deals(
            RuntimeOrigin::signed(account(CHARLIE)),
            bounded_vec![alice_proposal, alice_second_proposal, bob_proposal]
        ));
        assert_eq!(
            BalanceTable::<Test>::get(account(ALICE)),
            BalanceEntry::<u64> {
                free: 0,
                locked: 100
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
            BalanceTable::<Test>::get(account(CHARLIE)),
            BalanceEntry::<u64> {
                free: 10,
                locked: 300
            }
        );

        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(
                Event::<Test>::DealsPublished {
                    provider: account(CHARLIE),
                    deals: bounded_vec!(
                        PublishedDeal {
                            deal_id: alice_deal_id,
                            client: account(ALICE),
                        },
                        PublishedDeal {
                            deal_id: alice_second_deal_id,
                            client: account(ALICE),
                        },
                        PublishedDeal {
                            deal_id: bob_deal_id,
                            client: account(BOB),
                        }
                    )
                }
            ),]
        );
        assert!(PendingProposals::<Test>::get().contains(&alice_hash));
        assert!(PendingProposals::<Test>::get().contains(&bob_hash));
        assert!(DealsForBlock::<Test>::get(&alice_start_block).contains(&alice_deal_id));
        assert!(DealsForBlock::<Test>::get(&bob_start_block).contains(&bob_deal_id));
    });
}

#[test]
fn publish_storage_deals_with_deal_params() {
    new_test_ext().execute_with(|| {
        register_storage_provider(account(CHARLIE));
        let deal_params: OffchainDealParameters<u64, u64> = OffchainDealParameters {
            minimum_price_per_block: 4,
            deal_duration: OffchainDealDurationBound {
                lower: None,
                upper: None,
            },
        };
        // Publish deal params
        assert_ok!(StorageProvider::publish_deal_parameters(
            RuntimeOrigin::signed(account(CHARLIE)),
            deal_params
        ));
        // Flush events, checked by other test.
        System::reset_events();

        let alice_proposal = DealProposalBuilder::default()
            .client(ALICE)
            .provider(&CHARLIE)
            .signed(ALICE);
        let alice_start_block = 100;
        let alice_deal_id = 0;
        let alice_second_deal_id = 1;
        // We're not expecting for it to go through, but the call should not fail.
        let alice_second_proposal = DealProposalBuilder::default()
            .client(ALICE)
            .provider(&CHARLIE)
            .piece_size(37)
            .signed(ALICE);
        let bob_deal_id = 2;
        let bob_start_block = 130;
        let bob_proposal = DealProposalBuilder::default()
            .client(ALICE)
            .provider(&CHARLIE)
            .client(BOB)
            .start_block(bob_start_block)
            .end_block(135)
            .storage_price_per_block(10)
            .signed(BOB);

        let alice_hash = StorageProvider::hash_proposal(&alice_proposal.proposal);
        let bob_hash = StorageProvider::hash_proposal(&bob_proposal.proposal);

        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(ALICE)), 100);
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(BOB)), 70);
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(CHARLIE)), 310);
        System::reset_events();

        assert_ok!(StorageProvider::publish_storage_deals(
            RuntimeOrigin::signed(account(CHARLIE)),
            bounded_vec![alice_proposal, alice_second_proposal, bob_proposal]
        ));
        assert_eq!(
            BalanceTable::<Test>::get(account(ALICE)),
            BalanceEntry::<u64> {
                free: 0,
                locked: 100
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
            BalanceTable::<Test>::get(account(CHARLIE)),
            BalanceEntry::<u64> {
                free: 10,
                locked: 300
            }
        );

        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(
                Event::<Test>::DealsPublished {
                    provider: account(CHARLIE),
                    deals: bounded_vec!(
                        PublishedDeal {
                            deal_id: alice_deal_id,
                            client: account(ALICE),
                        },
                        PublishedDeal {
                            deal_id: alice_second_deal_id,
                            client: account(ALICE),
                        },
                        PublishedDeal {
                            deal_id: bob_deal_id,
                            client: account(BOB),
                        }
                    )
                }
            ),]
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
        publish_for_activation(
            1,
            DealProposalBuilder::default()
                .client(ALICE)
                .provider(&CHARLIE)
                .unsigned(),
        );

        let deals = bounded_vec![
            SectorDeal {
                sector_number: 1.into(),
                sector_expiry: 120,
                sector_type: RegisteredSealProof::StackedDRG2KiBV1P1,
                deal_ids: bounded_vec![1]
            },
            SectorDeal {
                sector_number: 2.into(),
                sector_expiry: 50,
                sector_type: RegisteredSealProof::StackedDRG2KiBV1P1,
                deal_ids: bounded_vec![]
            }
        ];

        assert_eq!(
            Ok(bounded_vec![
                Some(
                    Cid::from_str(
                        "baga6ea4seaqmruupwrxaeck7m3f5jtswpr7jv6bvwqeu5jinzjlcybh6er3ficq"
                    )
                    .unwrap()
                ),
                None,
            ]),
            crate::dispatchables::verify_deals_for_activation::<Test>(&account(CHARLIE), deals)
        );
    });
}

#[test]
fn verify_deals_for_activation_fails_with_different_provider() {
    new_test_ext().execute_with(|| {
        publish_for_activation(
            1,
            DealProposalBuilder::default()
                .client(ALICE)
                .provider(&CHARLIE)
                .provider(BOB)
                .unsigned(),
        );

        let deals = bounded_vec![SectorDealBuilder::default().build()];

        assert_noop!(
            crate::dispatchables::verify_deals_for_activation::<Test>(&account(CHARLIE), deals),
            Error::<Test>::InvalidProvider
        );
    });
}

#[test]
fn verify_deals_for_activation_fails_with_invalid_deal_state() {
    new_test_ext().execute_with(|| {
        publish_for_activation(
            1,
            DealProposalBuilder::default()
                .client(ALICE)
                .provider(&CHARLIE)
                .state(DealState::Active(ActiveDealState {
                    sector_number: 0.into(),
                    sector_start_block: 0,
                    last_updated_block: Some(10),
                    slash_block: None,
                }))
                .unsigned(),
        );

        let deals = bounded_vec![SectorDealBuilder::default().build()];

        assert_noop!(
            crate::dispatchables::verify_deals_for_activation::<Test>(&account(CHARLIE), deals),
            Error::<Test>::InvalidDealState
        );
    });
}

#[test]
fn verify_deals_for_activation_fails_deal_not_in_pending() {
    new_test_ext().execute_with(|| {
        // do not use `publish_for_activation` as it puts deal in PendingProposals
        Proposals::<Test>::insert(
            1,
            DealProposalBuilder::default()
                .client(ALICE)
                .provider(&CHARLIE)
                .unsigned(),
        );
        let deals = bounded_vec![SectorDealBuilder::default().build()];

        assert_noop!(
            crate::dispatchables::verify_deals_for_activation::<Test>(&account(CHARLIE), deals),
            Error::<Test>::DealNotPending
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
            DealProposalBuilder::default()
                .client(ALICE)
                .provider(&CHARLIE)
                .start_block(1)
                .unsigned(),
        );

        let deals = bounded_vec![SectorDealBuilder::default().build()];

        assert_noop!(
            crate::dispatchables::verify_deals_for_activation::<Test>(&account(CHARLIE), deals),
            Error::<Test>::StartBlockElapsed
        );
    });
}

#[test]
fn verify_deals_for_activation_fails_sector_expires_before_deal_ends() {
    new_test_ext().execute_with(|| {
        publish_for_activation(
            1,
            DealProposalBuilder::default()
                .client(ALICE)
                .provider(&CHARLIE)
                .start_block(10)
                .end_block(15)
                .unsigned(),
        );

        let deals = bounded_vec![SectorDealBuilder::default().sector_expiry(11).build()];

        assert_noop!(
            crate::dispatchables::verify_deals_for_activation::<Test>(&account(CHARLIE), deals),
            Error::<Test>::SectorExpiresBeforeDeal
        );
    });
}

#[test]
fn verify_deals_for_activation_fails_not_enough_space() {
    new_test_ext().execute_with(|| {
        publish_for_activation(
            1,
            DealProposalBuilder::default()
                .client(ALICE)
                .provider(&CHARLIE)
                .piece_size(1 << 10 /* 1 KiB */)
                .unsigned(),
        );
        publish_for_activation(
            2,
            DealProposalBuilder::default()
                .client(ALICE)
                .provider(&CHARLIE)
                .piece_size(3 << 10 /* 3 KiB */)
                .unsigned(),
        );
        // 1 KiB + 3KiB >= 2 KiB (sector size)

        let deals = bounded_vec![SectorDealBuilder::default()
            .deal_ids(bounded_vec![1, 2])
            .build()];

        assert_noop!(
            crate::dispatchables::verify_deals_for_activation::<Test>(&account(CHARLIE), deals),
            Error::<Test>::DealsTooLargeToFitIntoSector
        );
    });
}

#[test]
fn verify_deals_for_activation_fails_duplicate_deals() {
    new_test_ext().execute_with(|| {
        publish_for_activation(
            1,
            DealProposalBuilder::default()
                .client(ALICE)
                .provider(&CHARLIE)
                .unsigned(),
        );

        let deals = bounded_vec![SectorDealBuilder::default()
            .deal_ids(bounded_vec![1, 1])
            .build()];

        assert_noop!(
            crate::dispatchables::verify_deals_for_activation::<Test>(&account(CHARLIE), deals),
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
            crate::dispatchables::verify_deals_for_activation::<Test>(&account(CHARLIE), deals),
            Error::<Test>::DealNotFound
        );
    });
}

#[test]
fn activate_deals() {
    new_test_ext().execute_with(|| {
        register_storage_provider(account(CHARLIE));
        let alice_hash = publish_for_activation(
            1,
            DealProposalBuilder::default()
                .client(ALICE)
                .provider(&CHARLIE)
                .unsigned(),
        );

        let deals = bounded_vec![
            SectorDealBuilder::default().build(),
            SectorDealBuilder::default()
                .sector_number(2.into())
                .sector_expiry(50)
                .deal_ids(bounded_vec![])
                .build()
        ];

        let piece_cid =
            Cid::from_str("baga6ea4seaqgi5lnnv4wi5lnnv4wi5lnnv4wi5lnnv4wi5lnnv4wi5lnnv4wi5i")
                .unwrap();
        let commd_cid =
            Cid::from_str("baga6ea4seaqmruupwrxaeck7m3f5jtswpr7jv6bvwqeu5jinzjlcybh6er3ficq")
                .unwrap();
        assert_eq!(
            Ok(bounded_vec![
                ActiveSector {
                    active_deals: bounded_vec![ActiveDeal {
                        client: account(ALICE),
                        piece_cid: piece_cid,
                        piece_size: 128
                    }],
                    unsealed_cid: Some(commd_cid),
                },
                ActiveSector {
                    active_deals: bounded_vec![],
                    unsealed_cid: None
                }
            ]),
            crate::dispatchables::activate_deals::<Test>(&account(CHARLIE), deals, true)
        );
        assert!(!PendingProposals::<Test>::get().contains(&alice_hash));
    });
}

#[test]
fn activate_deals_fails_for_1_sector_but_succeeds_for_others() {
    new_test_ext().execute_with(|| {
        register_storage_provider(account(CHARLIE));
        let alice_hash = publish_for_activation(
            1,
            DealProposalBuilder::default()
                .client(ALICE)
                .provider(&CHARLIE)
                .unsigned(),
        );
        let _ = publish_for_activation(
            2,
            DealProposalBuilder::default()
                .client(ALICE)
                .provider(&CHARLIE)
                .unsigned(),
        );
        let deals = bounded_vec![
            SectorDealBuilder::default().build(),
            SectorDealBuilder::default()
                .sector_number(2.into())
                .sector_expiry(50)
                .deal_ids(bounded_vec![])
                .build(),
            SectorDealBuilder::default()
                .sector_number(3.into())
                .deal_ids(bounded_vec![1337])
                .build(),
            SectorDealBuilder::default()
                .sector_number(4.into())
                // force error by making expiry < start_block
                .sector_expiry(10)
                .deal_ids(bounded_vec![2])
                .build()
        ];

        let piece_cid =
            Cid::from_str("baga6ea4seaqgi5lnnv4wi5lnnv4wi5lnnv4wi5lnnv4wi5lnnv4wi5lnnv4wi5i")
                .unwrap();
        let commd_cid =
            Cid::from_str("baga6ea4seaqmruupwrxaeck7m3f5jtswpr7jv6bvwqeu5jinzjlcybh6er3ficq")
                .unwrap();
        assert_eq!(
            Ok(bounded_vec![
                ActiveSector {
                    active_deals: bounded_vec![ActiveDeal {
                        client: account(ALICE),
                        piece_cid: piece_cid,
                        piece_size: 128
                    }],
                    unsealed_cid: Some(commd_cid),
                },
                ActiveSector {
                    active_deals: bounded_vec![],
                    unsealed_cid: None
                }
            ]),
            crate::dispatchables::activate_deals::<Test>(&account(CHARLIE), deals, true)
        );
        assert!(!PendingProposals::<Test>::get().contains(&alice_hash));
    });
}

/// Creates a new deal and saves it in the Runtime Storage.
/// In addition to saving it to `Proposals::<T>` it also calculate's
/// it's hash and saves it to `PendingProposals::<T>`.
/// Behaves like `publish_storage_deals` without the validation and calling extrinsics.
fn publish_for_activation(deal_id: DealId, deal: DealProposalOf<Test>) -> H256 {
    let hash = StorageProvider::hash_proposal(&deal);
    let mut pending = PendingProposals::<Test>::get();
    pending.try_insert(hash).unwrap();
    PendingProposals::<Test>::set(pending);

    Proposals::<Test>::insert(deal_id, deal);
    hash
}

#[test]
fn verifies_deals_on_block_finalization() {
    new_test_ext().execute_with(|| {
        register_storage_provider(account(CHARLIE));
        let alice_start_block = 100;
        let alice_deal_id = 0;
        let alice_proposal = DealProposalBuilder::default()
            .client(ALICE)
            .provider(&CHARLIE)
            .start_block(alice_start_block)
            .end_block(alice_start_block + 10)
            .storage_price_per_block(5)
            .signed(ALICE);

        let bob_start_block = 130;
        let bob_deal_id = 1;
        let bob_proposal = DealProposalBuilder::default()
            .client(ALICE)
            .provider(&CHARLIE)
            .client(BOB)
            .start_block(bob_start_block)
            .end_block(bob_start_block + 5)
            .storage_price_per_block(10)
            .signed(BOB);

        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(ALICE)), 60);
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(BOB)), 70);
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(CHARLIE)), 310);
        let _ = StorageProvider::publish_storage_deals(
            RuntimeOrigin::signed(account(CHARLIE)),
            bounded_vec![alice_proposal, bob_proposal],
        );
        let _ = crate::dispatchables::activate_deals::<Test>(
            &account(CHARLIE),
            bounded_vec![SectorDeal {
                sector_number: 1.into(),
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
            BalanceTable::<Test>::get(account(CHARLIE)),
            BalanceEntry::<u64> {
                free: 110,
                locked: 200
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
            BalanceTable::<Test>::get(account(CHARLIE)),
            BalanceEntry::<u64> {
                free: 110,
                // 200 (locked) - 100 (lost collateral) = 100
                locked: 100
            }
        );

        assert!(!DealsForBlock::<Test>::get(&bob_start_block).contains(&bob_deal_id));
        assert_eq!(
            events(),
            [
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Rescinded { amount: 100 }),
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Withdraw {
                    who: StorageProvider::account_id(),
                    amount: 100
                }),
                RuntimeEvent::StorageProvider(Event::<Test>::DealSlashed {
                    deal_id: bob_deal_id,
                    amount: 100,
                    provider: account(CHARLIE),
                    client: account(BOB),
                })
            ]
        )
    });
}

#[test]
fn settle_deal_payments_not_found() {
    new_test_ext().execute_with(|| {
        assert_ok!(StorageProvider::settle_deal_payments(
            RuntimeOrigin::signed(account(ALICE)),
            bounded_vec!(0)
        ));

        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(Event::<Test>::DealsSettled {
                successful: bounded_vec!(),
                unsuccessful: bounded_vec!((0, DealSettlementError::DealNotFound))
            })]
        )
    });
}

#[test]
fn settle_deal_payments_early() {
    new_test_ext().execute_with(|| {
        register_storage_provider(account(CHARLIE));
        let alice_proposal = DealProposalBuilder::default()
            .client(ALICE)
            .provider(&CHARLIE)
            .signed(ALICE);

        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(ALICE)), 60);
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(CHARLIE)), 160);

        assert_ok!(StorageProvider::publish_storage_deals(
            RuntimeOrigin::signed(account(CHARLIE)),
            bounded_vec![alice_proposal]
        ));
        System::reset_events();

        assert_ok!(StorageProvider::settle_deal_payments(
            RuntimeOrigin::signed(account(ALICE)),
            bounded_vec!(0)
        ));

        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(Event::<Test>::DealsSettled {
                successful: bounded_vec!(),
                unsuccessful: bounded_vec!((0, DealSettlementError::EarlySettlement))
            })]
        )
    });
}

#[test]
fn settle_deal_payments_published() {
    new_test_ext().execute_with(|| {
        register_storage_provider(account(CHARLIE));
        let alice_proposal = DealProposalBuilder::default()
            .client(ALICE)
            .provider(&CHARLIE)
            .start_block(1)
            .end_block(11)
            .signed(ALICE);

        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(ALICE)), 60);
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(BOB)), 70);
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(CHARLIE)), 160);

        assert_ok!(StorageProvider::publish_storage_deals(
            RuntimeOrigin::signed(account(CHARLIE)),
            bounded_vec![alice_proposal]
        ));

        Proposals::<Test>::insert(
            1,
            DealProposalBuilder::default()
                .client(ALICE)
                .provider(&CHARLIE)
                .client(BOB)
                .start_block(1)
                .end_block(11)
                .storage_price_per_block(10)
                .unsigned(),
        );

        System::reset_events();

        assert_ok!(StorageProvider::settle_deal_payments(
            RuntimeOrigin::signed(account(ALICE)),
            bounded_vec!(0, 1, 2)
        ));

        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(Event::<Test>::DealsSettled {
                successful: bounded_vec!(),
                unsuccessful: bounded_vec!(
                    (0, DealSettlementError::DealNotActive),
                    (1, DealSettlementError::DealNotActive),
                    (2, DealSettlementError::DealNotFound)
                )
            })]
        )
    });
}

#[test]
fn settle_deal_payments_active_future_last_update() {
    new_test_ext().execute_with(|| {
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(ALICE)), 60);
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(CHARLIE)), 75);

        Proposals::<Test>::insert(
            0,
            DealProposalBuilder::default()
                .client(ALICE)
                .provider(&CHARLIE)
                .start_block(0)
                .end_block(10)
                .state(DealState::Active(ActiveDealState {
                    sector_number: 0.into(),
                    sector_start_block: 0,
                    last_updated_block: Some(10),
                    slash_block: None,
                }))
                .unsigned(),
        );
        System::reset_events();

        assert_ok!(StorageProvider::settle_deal_payments(
            RuntimeOrigin::signed(account(ALICE)),
            bounded_vec!(0)
        ));

        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(Event::<Test>::DealsSettled {
                successful: bounded_vec!(),
                unsuccessful: bounded_vec!((0, DealSettlementError::FutureLastUpdate))
            })]
        )
    });
}

#[test]
fn settle_deal_payments_active_corruption() {
    new_test_ext().execute_with(|| {
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(ALICE)), 60);
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(CHARLIE)), 75);

        Proposals::<Test>::insert(
            0,
            DealProposalBuilder::default()
                .client(ALICE)
                .provider(&CHARLIE)
                .start_block(0)
                .end_block(10)
                .state(DealState::Active(ActiveDealState {
                    sector_number: 0.into(),
                    sector_start_block: 0,
                    last_updated_block: Some(11),
                    slash_block: None,
                }))
                .unsigned(),
        );
        run_to_block(12);
        System::reset_events();

        assert_err!(
            StorageProvider::settle_deal_payments(
                RuntimeOrigin::signed(account(ALICE)),
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
        register_storage_provider(account(CHARLIE));
        let alice_proposal = DealProposalBuilder::default()
            .client(ALICE)
            .provider(&CHARLIE)
            .start_block(1)
            .end_block(11)
            .signed(ALICE);

        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(ALICE)), 60);
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(CHARLIE)), 160);

        assert_ok!(StorageProvider::publish_storage_deals(
            RuntimeOrigin::signed(account(CHARLIE)),
            bounded_vec![alice_proposal]
        ));

        Proposals::<Test>::mutate(0, |proposal| {
            if let Some(proposal) = proposal {
                proposal.state = DealState::Active(ActiveDealState {
                    sector_number: 0.into(),
                    sector_start_block: 0,
                    last_updated_block: None,
                    slash_block: None,
                })
            }
        });

        assert_eq!(
            Proposals::<Test>::get(0),
            Some(
                DealProposalBuilder::default()
                    .client(ALICE)
                    .provider(&CHARLIE)
                    .start_block(1)
                    .end_block(11)
                    .state(DealState::Active(ActiveDealState {
                        sector_number: 0.into(),
                        sector_start_block: 0,
                        last_updated_block: None,
                        slash_block: None,
                    }))
                    .unsigned()
            )
        );
        System::reset_events();

        run_to_block(6);

        assert_ok!(StorageProvider::settle_deal_payments(
            RuntimeOrigin::signed(account(ALICE)),
            bounded_vec!(0)
        ));

        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(Event::<Test>::DealsSettled {
                successful: bounded_vec!(SettledDealData {
                    deal_id: 0,
                    amount: 25,
                    client: account(ALICE),
                    provider: account(CHARLIE)
                }),
                unsuccessful: bounded_vec!()
            })]
        );

        assert_eq!(
            BalanceTable::<Test>::get(account(CHARLIE)),
            BalanceEntry::<u64> {
                free: 85, // 60 (from 160 - collateral) + 5 * 5 (price per block * n blocks)
                locked: 100
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
            Some(
                DealProposalBuilder::default()
                    .client(ALICE)
                    .provider(&CHARLIE)
                    .start_block(1)
                    .end_block(11)
                    .state(DealState::Active(ActiveDealState {
                        sector_number: 0.into(),
                        sector_start_block: 0,
                        last_updated_block: Some(6),
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
        register_storage_provider(account(CHARLIE));
        let alice_proposal = DealProposalBuilder::default()
            .client(ALICE)
            .provider(&CHARLIE)
            .start_block(1)
            .end_block(11)
            .signed(ALICE);

        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(ALICE)), 60);
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(CHARLIE)), 160);

        assert_ok!(StorageProvider::publish_storage_deals(
            RuntimeOrigin::signed(account(CHARLIE)),
            bounded_vec![alice_proposal]
        ));

        Proposals::<Test>::mutate(0, |proposal| {
            if let Some(proposal) = proposal {
                proposal.state = DealState::Active(ActiveDealState {
                    sector_number: 0.into(),
                    sector_start_block: 0,
                    last_updated_block: None,
                    slash_block: None,
                })
            }
        });

        assert_eq!(
            Proposals::<Test>::get(0),
            Some(
                DealProposalBuilder::default()
                    .client(ALICE)
                    .provider(&CHARLIE)
                    .start_block(1)
                    .end_block(11)
                    .state(DealState::Active(ActiveDealState {
                        sector_number: 0.into(),
                        sector_start_block: 0,
                        last_updated_block: None,
                        slash_block: None,
                    }))
                    .unsigned()
            )
        );

        System::reset_events();

        // Deal is finished
        run_to_block(12);

        assert_ok!(StorageProvider::settle_deal_payments(
            RuntimeOrigin::signed(account(ALICE)),
            bounded_vec!(0)
        ));

        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(Event::<Test>::DealsSettled {
                successful: bounded_vec!(SettledDealData {
                    deal_id: 0,
                    amount: 50,
                    client: account(ALICE),
                    provider: account(CHARLIE)
                }),
                unsuccessful: bounded_vec!()
            })]
        );

        assert_eq!(
            BalanceTable::<Test>::get(account(CHARLIE)),
            BalanceEntry::<u64> {
                free: 160 + 5 * 10, // 160 (from 160 - collateral + returned collateral (not slashed)) + (price per block * n blocks)
                locked: 0
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

#[test]
fn test_lock_funds() {
    new_test_ext().execute_with(|| {
        assert_eq!(
            <Test as CurrencyProvider>::Currency::total_balance(&account(CHARLIE)),
            50_000
        );
        assert_ok!(StorageProvider::add_balance(
            RuntimeOrigin::signed(account(CHARLIE)),
            90
        ));
        assert_eq!(
            <Test as CurrencyProvider>::Currency::total_balance(&account(CHARLIE)),
            49_910
        );
        assert_ok!(lock_funds::<Test>(&account(CHARLIE), 25));
        assert_eq!(
            BalanceTable::<Test>::get(account(CHARLIE)),
            BalanceEntry::<u64> {
                free: 65,
                locked: 25,
            }
        );

        assert_ok!(lock_funds::<Test>(&account(CHARLIE), 65));
        assert_eq!(
            BalanceTable::<Test>::get(account(CHARLIE)),
            BalanceEntry::<u64> {
                free: 0,
                locked: 90,
            }
        );

        assert_err!(
            lock_funds::<Test>(&account(CHARLIE), 25),
            Error::<Test>::InsufficientFreeFunds
        );

        assert_eq!(
            BalanceTable::<Test>::get(account(CHARLIE)),
            BalanceEntry::<u64> {
                free: 0,
                locked: 90,
            }
        );
    });
}

#[test]
fn test_unlock_funds() {
    new_test_ext().execute_with(|| {
        assert_eq!(
            <Test as CurrencyProvider>::Currency::total_balance(&account(CHARLIE)),
            50_000
        );
        // We can't get all 100, otherwise the account would be reaped
        assert_ok!(StorageProvider::add_balance(
            RuntimeOrigin::signed(account(CHARLIE)),
            90
        ));
        assert_eq!(
            <Test as CurrencyProvider>::Currency::total_balance(&account(CHARLIE)),
            49_910
        );
        assert_ok!(lock_funds::<Test>(&account(CHARLIE), 90));
        assert_eq!(
            BalanceTable::<Test>::get(account(CHARLIE)),
            BalanceEntry::<u64> {
                free: 0,
                locked: 90,
            }
        );

        assert_ok!(unlock_funds::<Test>(&account(CHARLIE), 30));
        assert_eq!(
            BalanceTable::<Test>::get(account(CHARLIE)),
            BalanceEntry::<u64> {
                free: 30,
                locked: 60,
            }
        );

        assert_ok!(unlock_funds::<Test>(&account(CHARLIE), 60));
        assert_eq!(
            BalanceTable::<Test>::get(account(CHARLIE)),
            BalanceEntry::<u64> {
                free: 90,
                locked: 0,
            }
        );

        assert_err!(
            unlock_funds::<Test>(&account(CHARLIE), 60),
            Error::<Test>::InsufficientLockedFunds
        );
        assert_eq!(
            BalanceTable::<Test>::get(account(CHARLIE)),
            BalanceEntry::<u64> {
                free: 90,
                locked: 0,
            }
        );
    });
}

#[test]
fn slash_and_burn_acc() {
    new_test_ext().execute_with(|| {
        assert_eq!(
            <Test as CurrencyProvider>::Currency::total_issuance(),
            150_000
        );
        assert_ok!(StorageProvider::add_balance(
            RuntimeOrigin::signed(account(CHARLIE)),
            75
        ));

        System::reset_events();

        assert_ok!(lock_funds::<Test>(&account(CHARLIE), 10));
        assert_ok!(slash_and_burn::<Test>(&account(CHARLIE), 10));

        assert_eq!(
            events(),
            [
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Rescinded { amount: 10 }),
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Withdraw {
                    who: StorageProvider::account_id(),
                    amount: 10
                }),
            ]
        );
        assert_eq!(
            <Test as CurrencyProvider>::Currency::total_issuance(),
            149_990
        );

        assert_eq!(
            BalanceTable::<Test>::get(account(CHARLIE)),
            BalanceEntry::<u64> {
                free: 65,
                locked: 0,
            }
        );

        assert_err!(
            slash_and_burn::<Test>(&account(CHARLIE), 10),
            Error::<Test>::InsufficientLockedFunds
        );
        assert_eq!(
            <Test as CurrencyProvider>::Currency::total_issuance(),
            149_990
        );
    });
}

#[test]
fn on_sector_terminate_unknown_deals() {
    new_test_ext().execute_with(|| {
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(CHARLIE)), 75);
        System::reset_events();

        assert_ok!(crate::dispatchables::on_sectors_terminate::<Test>(
            &account(CHARLIE),
            bounded_vec![0.into()],
        ));

        assert_eq!(events(), []);
    });
}

#[test]
fn on_sector_terminate_deal_not_found() {
    new_test_ext().execute_with(|| {
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(CHARLIE)), 75);
        System::reset_events();

        let storage_provider = account(CHARLIE);
        let sector_number = 0.into();
        let sector_deal_ids: BoundedVec<_, ConstU32<MAX_DEALS_PER_SECTOR>> = bounded_vec![1];

        SectorDeals::<Test>::insert((storage_provider.clone(), sector_number), sector_deal_ids);

        assert_err!(
            crate::dispatchables::on_sectors_terminate::<Test>(
                &storage_provider,
                bounded_vec![sector_number]
            ),
            Error::<Test>::DealNotFound
        );

        assert_eq!(events(), []);
    });
}

#[test]
fn on_sector_terminate_invalid_caller() {
    new_test_ext().execute_with(|| {
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(CHARLIE)), 75);
        System::reset_events();

        let sector_number = 0.into();
        let sector_deal_ids: BoundedVec<_, ConstU32<MAX_DEALS_PER_SECTOR>> = bounded_vec![1];

        SectorDeals::<Test>::insert((account(CHARLIE), sector_number), sector_deal_ids);
        Proposals::<Test>::insert(
            1,
            DealProposalBuilder::default()
                .client(ALICE)
                .provider(&CHARLIE)
                .client(BOB)
                .unsigned(),
        );

        assert_ok!(crate::dispatchables::on_sectors_terminate::<Test>(
            &account(BOB),
            bounded_vec![sector_number]
        ),);

        assert_eq!(events(), []);
    });
}

#[test]
fn on_sector_terminate_not_active() {
    new_test_ext().execute_with(|| {
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(CHARLIE)), 75);
        System::reset_events();

        let storage_provider = account(CHARLIE);
        let sector_number = 0.into();
        let sector_deal_ids: BoundedVec<_, ConstU32<MAX_DEALS_PER_SECTOR>> = bounded_vec![1];

        SectorDeals::<Test>::insert((storage_provider.clone(), sector_number), sector_deal_ids);
        Proposals::<Test>::insert(
            1,
            DealProposalBuilder::default()
                .client(ALICE)
                .provider(&CHARLIE)
                .client(BOB)
                .start_block(0)
                .end_block(10)
                .storage_price_per_block(10)
                .unsigned(),
        );

        assert_err!(
            crate::dispatchables::on_sectors_terminate::<Test>(
                &storage_provider,
                bounded_vec![sector_number]
            ),
            Error::<Test>::DealIsNotActive
        );

        assert_eq!(events(), []);
    });
}

#[test]
fn on_sector_terminate_active() {
    new_test_ext().execute_with(|| {
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(BOB)), 75);
        let _ = StorageProvider::add_balance(RuntimeOrigin::signed(account(CHARLIE)), 160);

        let storage_provider = account(CHARLIE);
        let sector_number = 0.into();
        let sector_deal_ids: BoundedVec<_, ConstU32<MAX_DEALS_PER_SECTOR>> = bounded_vec![1];
        let deal_proposal = DealProposalBuilder::default()
            .client(ALICE)
            .provider(&CHARLIE)
            .client(BOB)
            .start_block(0)
            .end_block(10)
            .storage_price_per_block(5)
            .state(DealState::Active(ActiveDealState::new(sector_number, 0)))
            .unsigned();

        assert_ok!(lock_funds::<Test>(&account(BOB), 5 * 10));
        assert_ok!(lock_funds::<Test>(&storage_provider, 100));

        let hash_proposal = StorageProvider::hash_proposal(&deal_proposal);
        let mut pending = PendingProposals::<Test>::get();
        pending
            .try_insert(hash_proposal)
            .expect("should have enough space");
        PendingProposals::<Test>::set(pending);

        SectorDeals::<Test>::insert((storage_provider.clone(), sector_number), sector_deal_ids);
        Proposals::<Test>::insert(1, deal_proposal);

        System::reset_events();

        assert_ok!(crate::dispatchables::on_sectors_terminate::<Test>(
            &storage_provider,
            bounded_vec![sector_number],
        ));

        assert_eq!(
            BalanceTable::<Test>::get(&account(BOB)),
            BalanceEntry {
                free: 70,  // unlocked funds - 5 for the storage payment of a single block
                locked: 0, // unlocked
            }
        );

        assert_eq!(
            BalanceTable::<Test>::get(&storage_provider),
            BalanceEntry {
                free: 65,  // the original 60 + 5 for the storage payment of a single block
                locked: 0, // lost the 100 collateral
            }
        );

        assert_eq!(
            events(),
            [
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Rescinded { amount: 100 }),
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Withdraw {
                    who: StorageProvider::account_id(),
                    amount: 100
                }),
                RuntimeEvent::StorageProvider(Event::<Test>::DealTerminated {
                    deal_id: 1,
                    client: account(BOB),
                    provider: account(CHARLIE)
                })
            ]
        );
        assert!(PendingProposals::<Test>::get().is_empty());
        assert!(!Proposals::<Test>::contains_key(1));
        assert_eq!(
            <Test as CurrencyProvider>::Currency::total_issuance(),
            149900
        );
    });
}

#[test]
fn publish_deal_parameters() {
    new_test_ext().execute_with(|| {
        let storage_provider = account(CHARLIE);
        register_storage_provider(storage_provider.clone());

        let offchain_deal_params: OffchainDealParameters<u64, u64> = OffchainDealParameters {
            minimum_price_per_block: 1_000,
            deal_duration: OffchainDealDurationBound {
                lower: Some(3),  // Chain minimum = 2
                upper: Some(29), // Chain maximum = 30
            },
        };
        let deal_params = offchain_deal_params
            .clone()
            .validate(
                <<Test as Config>::MinDealDuration as Get<u64>>::get(),
                <<Test as Config>::MaxDealDuration as Get<u64>>::get(),
            )
            .expect("Seamless conversion");

        // Run extrinsic
        assert_ok!(StorageProvider::publish_deal_parameters(
            RuntimeOrigin::signed(storage_provider.clone()),
            offchain_deal_params
        ));

        // Check events
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(
                Event::<Test>::DealParametersUpdated {
                    provider: storage_provider.clone(),
                    deal_parameters: deal_params.clone()
                }
            )]
        );

        // Check storage map
        assert_eq!(
            SPDealParameters::<Test>::try_get(&storage_provider),
            Ok(deal_params)
        );

        // Re-insert different deal parameters
        let offchain_deal_params_2: OffchainDealParameters<u64, u64> = OffchainDealParameters {
            minimum_price_per_block: 10_000,
            deal_duration: OffchainDealDurationBound {
                lower: Some(4),  // Chain minimum = 2
                upper: Some(28), // Chain maximum = 30
            },
        };

        let deal_params_2 = offchain_deal_params_2
            .clone()
            .validate(
                <<Test as Config>::MinDealDuration as Get<u64>>::get(),
                <<Test as Config>::MaxDealDuration as Get<u64>>::get(),
            )
            .expect("Seamless conversion");

        // Run extrinsic
        assert_ok!(StorageProvider::publish_deal_parameters(
            RuntimeOrigin::signed(storage_provider.clone()),
            offchain_deal_params_2.clone()
        ));

        // Check events
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(
                Event::<Test>::DealParametersUpdated {
                    provider: storage_provider.clone(),
                    deal_parameters: deal_params_2.clone()
                }
            )]
        );

        // Check storage map
        assert_eq!(
            SPDealParameters::<Test>::try_get(&storage_provider),
            Ok(deal_params_2)
        );
    });
}

#[test]
fn remove_deal_parameters() {
    new_test_ext().execute_with(|| {
        let storage_provider = account(CHARLIE);
        register_storage_provider(storage_provider.clone());

        let offchain_deal_params: OffchainDealParameters<u64, u64> = OffchainDealParameters {
            minimum_price_per_block: 1_000,
            deal_duration: OffchainDealDurationBound {
                lower: Some(3),  // Chain minimum = 2
                upper: Some(29), // Chain maximum = 30
            },
        };
        let deal_params = offchain_deal_params
            .clone()
            .validate(
                <<Test as Config>::MinDealDuration as Get<u64>>::get(),
                <<Test as Config>::MaxDealDuration as Get<u64>>::get(),
            )
            .expect("Seamless conversion");

        // Run extrinsic
        assert_ok!(StorageProvider::publish_deal_parameters(
            RuntimeOrigin::signed(storage_provider.clone()),
            offchain_deal_params.clone()
        ));

        // Check events
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(
                Event::<Test>::DealParametersUpdated {
                    provider: storage_provider.clone(),
                    deal_parameters: deal_params.clone()
                }
            )]
        );

        // Check storage map
        assert_eq!(
            SPDealParameters::<Test>::try_get(&storage_provider),
            Ok(deal_params)
        );

        // Remove deal parameters
        assert_ok!(StorageProvider::remove_deal_parameters(
            RuntimeOrigin::signed(storage_provider.clone())
        ));

        // Check storage map
        assert!(SPDealParameters::<Test>::try_get(&storage_provider).is_err());

        // Check events
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(
                Event::<Test>::DealParametersRemoved {
                    provider: storage_provider.clone(),
                }
            )]
        );
    });
}
