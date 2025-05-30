use frame_support::{dispatch::DispatchResult, ensure};
use frame_system::{ensure_signed, pallet_prelude::OriginFor};
use primitives::pallets::StorageProviderValidation;

use crate::{Config, Error, Event, Pallet, SPDealParameters};

pub fn remove_deal_parameters<T>(origin: OriginFor<T>) -> DispatchResult
where
    T: Config,
{
    let provider = ensure_signed(origin)?;

    ensure!(
        T::StorageProviderValidation::is_registered_storage_provider(&provider),
        Error::<T>::StorageProviderNotRegistered
    );
    ensure!(
        SPDealParameters::<T>::contains_key(&provider),
        Error::<T>::NoDealParamsToRemove
    );

    SPDealParameters::<T>::remove(&provider);
    Pallet::<T>::deposit_event(Event::<T>::DealParametersRemoved { provider });

    Ok(())
}
