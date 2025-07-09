use frame_support::dispatch::DispatchResult;
use frame_system::{ensure_signed, pallet_prelude::OriginFor};

use crate::{Config, Error, Event, Pallet, SPDealParameters, StorageProviders};

pub fn deregister_storage_provider<T>(origin: OriginFor<T>) -> DispatchResult
where
    T: Config,
{
    let owner = ensure_signed(origin)?;
    let sp = match StorageProviders::<T>::try_get(&owner) {
        Ok(sp) => sp,
        Err(..) => {
            log::error!("Storage Provider {owner:?} is not registered");
            return Err(Error::<T>::StorageProviderNotRegistered.into());
        }
    };

    // Check pre-committed sectors, if there are any, block deregistration.
    if sp.pre_committed_sectors.len() > 0 {
        log::error!("{owner:?} has pre-committed sectors, cannot deregister");
        return Err(Error::<T>::SPHasPreCommittedSectors.into());
    }

    // Get live sectors for registered SP
    // Having live sectors == deals still active
    let live_sectors = sp
        .get_deadlines()
        .due
        .iter()
        .map(|deadline| {
            if deadline.live_sectors > 0 {
                log::info!("{deadline:#?}");
            }
            deadline.live_sectors
        })
        .sum::<u64>();

    if live_sectors > 0 {
        log::error!("{owner:?} still has {live_sectors} live sectors, cannot deregister");
        return Err(Error::<T>::SPHasActiveDeals.into());
    }

    StorageProviders::<T>::remove(&owner);
    SPDealParameters::<T>::remove(&owner);

    // Emit event
    Pallet::<T>::deposit_event(Event::StorageProviderDeregistered {
        owner,
        info: sp.info,
    });
    Ok(())
}
