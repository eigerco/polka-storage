use frame_support::{
    dispatch::DispatchResult,
    ensure,
    traits::{Currency, ExistenceRequirement::AllowDeath},
};
use frame_system::{ensure_signed, pallet_prelude::OriginFor};
use sp_runtime::{traits::CheckedSub, ArithmeticError};

use crate::{BalanceOf, BalanceTable, Config, Error, Event, Pallet, LOG_TARGET};

pub fn withdraw_balance<T>(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult
where
    T: Config,
{
    let caller = ensure_signed(origin)?;

    BalanceTable::<T>::try_mutate(&caller, |balance| -> DispatchResult {
        ensure!(balance.free >= amount, {
            log::error!(target: LOG_TARGET, "withdraw_balance: not enough free balance {:?} < {:?}", balance.free, amount);
            Error::<T>::InsufficientFreeFunds
        });

        balance.free = balance
            .free
            .checked_sub(&amount)
            .ok_or(ArithmeticError::Underflow)?;
        // The Market Pallet account will be reaped if no one is participating in the market.
        T::Currency::transfer(&Pallet::<T>::account_id(), &caller, amount, AllowDeath)?;

        Ok(())
    })?;

    Pallet::<T>::deposit_event(Event::<T>::BalanceWithdrawn {
        who: caller.clone(),
        amount,
    });

    Ok(())
}
