use frame_support::{
    assert_noop, assert_ok,
    sp_runtime::{ArithmeticError, TokenError},
};

use crate::{mock::*, BalanceEntry, BalanceTable, Error, Event};

#[test]
fn initial_state() {
    new_test_ext().execute_with(|| {
        assert_eq!(Balances::free_balance(Market::account_id()), 0);
        assert_eq!(
            BalanceTable::<Test>::get(ALICE),
            BalanceEntry::<u64> { free: 0, locked: 0 }
        );
    });
}

#[test]
fn basic_end_to_end_works() {
    new_test_ext().execute_with(|| {
        // Adds funds from an account to the Market
        assert_ok!(Market::add_balance(RuntimeOrigin::signed(ALICE), 10));
        assert_eq!(Balances::free_balance(Market::account_id()), 10);
        assert_eq!(Balances::free_balance(ALICE), INITIAL_FUNDS - 10);
        assert_eq!(
            BalanceTable::<Test>::get(ALICE),
            BalanceEntry::<u64> {
                free: 10,
                locked: 0,
            }
        );

        // Is able to withdraw added funds back
        assert_ok!(Market::withdraw_balance(RuntimeOrigin::signed(ALICE), 10));
        assert_eq!(Balances::free_balance(Market::account_id()), 0);
        assert_eq!(Balances::free_balance(ALICE), INITIAL_FUNDS);
        assert_eq!(
            BalanceTable::<Test>::get(ALICE),
            BalanceEntry::<u64> { free: 0, locked: 0 }
        );
    });
}

#[test]
fn adds_balance() {
    new_test_ext().execute_with(|| {
        assert_ok!(Market::add_balance(RuntimeOrigin::signed(ALICE), 10));
        assert_eq!(Balances::free_balance(Market::account_id()), 10);
        assert_eq!(Balances::free_balance(ALICE), INITIAL_FUNDS - 10);
        assert_eq!(
            BalanceTable::<Test>::get(ALICE),
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
                    from: ALICE,
                    to: Market::account_id(),
                    amount: 10
                }),
                RuntimeEvent::Market(Event::<Test>::BalanceAdded {
                    who: ALICE,
                    amount: 10
                })
            ]
        );

        // Makes sure other accounts are unaffected
        assert_eq!(
            BalanceTable::<Test>::get(BOB),
            BalanceEntry::<u64> { free: 0, locked: 0 }
        );
    });
}

#[test]
fn fails_to_add_balance_insufficient_funds() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Market::add_balance(RuntimeOrigin::signed(ALICE), INITIAL_FUNDS + 1),
            TokenError::FundsUnavailable,
        );
    });
}

#[test]
fn fails_to_add_balance_overflow() {
    new_test_ext().execute_with(|| {
        // Hard to do this without setting it explicitly in the map
        BalanceTable::<Test>::set(
            BOB,
            BalanceEntry::<u64> {
                free: u64::MAX,
                locked: 0,
            },
        );

        assert_noop!(
            Market::add_balance(RuntimeOrigin::signed(BOB), 1),
            ArithmeticError::Overflow
        );
    });
}

#[test]
fn withdraws_balance() {
    new_test_ext().execute_with(|| {
        let _ = Market::add_balance(RuntimeOrigin::signed(ALICE), 10);
        System::reset_events();

        assert_ok!(Market::withdraw_balance(RuntimeOrigin::signed(ALICE), 10));
        assert_eq!(Balances::free_balance(Market::account_id()), 0);
        assert_eq!(Balances::free_balance(ALICE), INITIAL_FUNDS);
        assert_eq!(
            BalanceTable::<Test>::get(ALICE),
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
                    to: ALICE,
                    amount: 10
                }),
                RuntimeEvent::Market(Event::<Test>::BalanceWithdrawn {
                    who: ALICE,
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
            Market::withdraw_balance(RuntimeOrigin::signed(BOB), 10),
            Error::<Test>::InsufficientFreeFunds
        );

        assert_eq!(events(), []);
    });
}
