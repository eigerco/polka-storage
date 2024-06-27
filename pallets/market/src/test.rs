use frame_support::{
    assert_noop, assert_ok,
    sp_runtime::{bounded_vec, ArithmeticError, TokenError},
};

use crate::{mock::*, BalanceEntry, BalanceTable, DealProposal, DealState, Error, Event};

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
                state: DealState::Unpublished,
            },
        );
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
                start_block: 130,
                end_block: 135,
                storage_price_per_block: 10,
                provider_collateral: 15,
                state: DealState::Unpublished,
            },
        );

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
                    deal_id: 0,
                    client: account(ALICE),
                    provider: account(PROVIDER),
                }),
                RuntimeEvent::Market(Event::<Test>::DealPublished {
                    deal_id: 1,
                    client: account(BOB),
                    provider: account(PROVIDER),
                }),
            ]
        );
    });
}
