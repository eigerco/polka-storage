use frame_support::{dispatch::DispatchResult, ensure, pallet_prelude::*, LOG_TARGET};
use frame_system::pallet_prelude::*;

use crate::{
    deal::parameters::OffchainDealParameters, BalanceOf, Config, Error, Event, Pallet,
    SPDealParameters,
};

pub fn publish_deal_parameters<T>(
    origin: OriginFor<T>,
    deal_parameters: OffchainDealParameters<BalanceOf<T>, BlockNumberFor<T>>,
) -> DispatchResult
where
    T: Config,
{
    let provider = ensure_signed(origin)?;
    ensure!(
        Pallet::<T>::is_registered_storage_provider(&provider),
        Error::<T>::StorageProviderNotRegistered
    );
    let deal_parameters = deal_parameters
        .validate(T::MinDealDuration::get(), T::MaxDealDuration::get())
        .map_err(|e| {
            log::error!(target: LOG_TARGET, "{e}");
            Error::<T>::InvalidDealParametersSubmitted
        })?;
    // Update deal parameters
    SPDealParameters::<T>::mutate(&provider, |params| {
        let _ = params.insert(deal_parameters.clone());
    });

    Pallet::<T>::deposit_event(Event::<T>::DealParametersUpdated {
        provider,
        deal_parameters,
    });

    Ok(())
}
