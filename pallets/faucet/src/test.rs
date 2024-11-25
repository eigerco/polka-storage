use frame_support::{assert_err, assert_ok};
use frame_system::Event as SystemEvent;
use pallet_balances::Event as BalanceEvent;

use crate::{mock::*, Error, Event};

#[test]
fn drip() {
    new_test_ext().execute_with(|| {
        let account = account::<Test>(ALICE);
        assert_ok!(Faucet::drip(RuntimeOrigin::none(), account.clone()));

        // The initial drip should create the account
        assert_eq!(
            events(),
            [
                RuntimeEvent::Balances(BalanceEvent::Issued {
                    amount: <Test as crate::Config>::FaucetDripAmount::get()
                }),
                RuntimeEvent::Balances(BalanceEvent::Deposit {
                    who: account.clone(),
                    amount: <Test as crate::Config>::FaucetDripAmount::get()
                }),
                RuntimeEvent::System(SystemEvent::NewAccount {
                    account: account.clone()
                }),
                RuntimeEvent::Balances(BalanceEvent::Endowed {
                    account: account.clone(),
                    free_balance: <Test as crate::Config>::FaucetDripAmount::get()
                }),
                RuntimeEvent::Faucet(Event::Dripped {
                    who: account.clone(),
                    when: System::block_number()
                })
            ]
        );

        assert_eq!(
            Balances::free_balance(account.clone()),
            <Test as crate::Config>::FaucetDripAmount::get()
        );
    });
}

#[test]
fn early_drip_fails() {
    new_test_ext().execute_with(|| {
        let account = account::<Test>(ALICE);
        Faucet::drip(RuntimeOrigin::none(), account.clone())
            .expect("first drip should always succeed");

        // Run to block_number + faucet_delay
        run_to_block(System::block_number() + <Test as crate::Config>::FaucetDripDelay::get() - 1);

        // Check that dripping at the same block is blocked
        assert_err!(
            Faucet::drip(RuntimeOrigin::none(), account.clone()),
            Error::<Test>::FaucetUsedRecently
        );
    });
}

#[test]
fn drip_delay_succeeds() {
    new_test_ext().execute_with(|| {
        let account = account::<Test>(ALICE);
        Faucet::drip(RuntimeOrigin::none(), account.clone())
            .expect("first drip should always succeed");

        // We've tested this scenario so we can reset the events
        System::reset_events();

        // Run to block_number + faucet_delay
        run_to_block(System::block_number() + <Test as crate::Config>::FaucetDripDelay::get());

        // Rerun drip, should be successful
        assert_ok!(Faucet::drip(RuntimeOrigin::none(), account.clone()));

        // Expecting less events because no new account is created
        assert_eq!(
            events(),
            [
                RuntimeEvent::Balances(BalanceEvent::Issued {
                    amount: <Test as crate::Config>::FaucetDripAmount::get()
                }),
                RuntimeEvent::Balances(BalanceEvent::Deposit {
                    who: account.clone(),
                    amount: <Test as crate::Config>::FaucetDripAmount::get()
                }),
                RuntimeEvent::Faucet(Event::Dripped {
                    who: account.clone(),
                    when: System::block_number()
                })
            ]
        );

        assert_eq!(
            Balances::free_balance(account),
            <Test as crate::Config>::FaucetDripAmount::get() * 2
        );
    });
}
