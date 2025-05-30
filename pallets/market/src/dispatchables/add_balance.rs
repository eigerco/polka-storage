use frame_support::{
    dispatch::DispatchResult,
    traits::{Currency, ExistenceRequirement::KeepAlive},
};
use frame_system::{ensure_signed, pallet_prelude::OriginFor};
use primitives::configs::BalanceOf;
use sp_runtime::{traits::CheckedAdd, ArithmeticError};

use crate::{BalanceTable, Config, Event, Pallet};

pub fn add_balance<T>(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult
where
    T: Config,
{
    let caller = ensure_signed(origin)?;

    BalanceTable::<T>::try_mutate(&caller, |balance| -> DispatchResult {
        balance.free = balance
            .free
            .checked_add(&amount)
            .ok_or(ArithmeticError::Overflow)?;
        T::Currency::transfer(&caller, &Pallet::<T>::account_id(), amount, KeepAlive)?;

        Ok(())
    })?;

    Pallet::<T>::deposit_event(Event::<T>::BalanceAdded {
        who: caller.clone(),
        amount,
    });

    Ok(())
}
