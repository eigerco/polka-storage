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

        // Check that dripping at the same block is blocked
        assert_err!(
            Faucet::drip(RuntimeOrigin::none(), account.clone()),
            Error::<Test>::FaucetUsedRecently
        );

        // Run to block_number + faucet_delay
        run_to_block(System::block_number() + <Test as crate::Config>::FaucetDripDelay::get());

        // Rerun drip, should be successful
        assert_ok!(Faucet::drip(RuntimeOrigin::none(), account.clone()));

        // Expecting less events because no new account is created
        assert_eq!(
            events(),
            [
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
            Balances::free_balance(account.clone()),
            <Test as crate::Config>::FaucetDripAmount::get() * 2
        );
    });
}
