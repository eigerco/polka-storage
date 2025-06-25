use frame_support::dispatch::DispatchResult;
use frame_system::{ensure_signed, pallet_prelude::OriginFor};

use crate::{BalanceOf, Config};

#[deprecated(note = "This function is no-op and will be removed in the future.")]
pub fn add_balance<T>(origin: OriginFor<T>, _amount: BalanceOf<T>) -> DispatchResult
where
    T: Config,
{
    ensure_signed(origin)?;
    Ok(())
}
