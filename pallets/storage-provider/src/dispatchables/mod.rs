mod declare_faults;
mod declare_faults_recovered;
mod pre_commit_sectors;
mod prove_commit_sectors;
mod register_storage_provider;
mod submit_windowed_post;
mod terminate_sectors;

pub use declare_faults::declare_faults;
pub use declare_faults_recovered::declare_faults_recovered;
pub use pre_commit_sectors::pre_commit_sectors;
pub use prove_commit_sectors::prove_commit_sectors;
pub use register_storage_provider::register_storage_provider;
pub use submit_windowed_post::submit_windowed_post;
pub use terminate_sectors::terminate_sectors;

extern crate alloc;

use alloc::vec::Vec;

use frame_support::pallet_prelude::{DispatchError, One};
use frame_system::pallet_prelude::BlockNumberFor;
use primitives::{
    configs::BalanceOf,
    pallets::Market,
    randomness::{draw_randomness, AuthorVrfHistory, DomainSeparationTag},
};
use sp_core::Get;

use crate::{Config, Error, StorageProviders, LOG_TARGET};

/// Calculate the required pre commit deposit amount
pub(crate) fn calculate_pre_commit_deposit<T>() -> BalanceOf<T>
where
    T: Config,
{
    BalanceOf::<T>::one() // TODO(@aidan46, #106, 2024-06-24): Set a logical value or calculation
}

/// Processes terminations for the given account (should be a registered SP).
/// Clears all early terminations and calls `on_sectors_terminate` when finished.
pub(crate) fn process_early_terminations<T>(
    current_block: BlockNumberFor<T>,
    owner: &T::AccountId,
) -> Result<(), Error<T>>
where
    T: Config,
{
    let mut state =
        StorageProviders::<T>::try_get(owner).map_err(|_| Error::<T>::StorageProviderNotFound)?;
    let result = state
        .pop_early_terminations(
            T::AddressedPartitionsMax::get(),
            T::AddressedSectorsMax::get(),
        )
        .map_err(|e| Error::<T>::GeneralPalletError(e))?;

    // Nothing to do, don't waste any time.
    // This can happen if we end up processing early terminations
    // before the cron callback fires.
    if result.is_empty() {
        log::info!(target: LOG_TARGET, "no early terminations");
        return Ok(());
    }

    let mut sectors_with_data = Vec::new();
    // Check whether sectors have expired, if not push to sectors_with_data for later processing.
    // Process in Market pallet `on_sectors_terminate`.
    // TODO(@aidan46, #452, 2024-10-14): Figure out economics to apply early termination penalty.
    for (&expiry, sector_numbers) in result.sectors.iter() {
        for sector_number in sector_numbers {
            // I am not 100% sure this is correct. In FC they use deal weight to determine.
            // Deal weight is a function of space times the duration of a deal.
            if expiry < current_block {
                sectors_with_data.push(*sector_number);
            }
        }
    }

    // Terminate deals
    let terminated_data = sectors_with_data.try_into().expect(
                "The sectors in the result can never be more than MAX_DEALS_PER_SECTOR due to previous bounds, this should not fail",
            );

    T::Market::on_sectors_terminate(owner, terminated_data)
        .map_err(|_| Error::<T>::CouldNotTerminateDeals)?;

    // Update storage provider state
    StorageProviders::<T>::insert(owner, state);

    Ok(())
}

/// Get randomness from the chain and process it with domain separation.
pub(crate) fn get_randomness<T: Config>(
    personalization: DomainSeparationTag,
    block_number: BlockNumberFor<T>,
    entropy: &[u8],
) -> Result<[u8; 32], DispatchError> {
    // Get randomness from chain
    let Some(randomness) = T::AuthorVrfHistory::author_vrf_history(block_number) else {
        return Err(Error::<T>::MissingAuthorVRF.into());
    };

    // Converting block_height to the type accepted by draw_randomness
    let block_number = block_number.try_into().map_err(|_| {
        log::error!(target: LOG_TARGET, "get_randomness: failed to convert block_height to u64");
        Error::<T>::ConversionError
    })?;

    // HACK: convert an unsized slice to a sized one
    // SAFETY: we know that the output is 32 bytes since we control the chain config
    let mut sized_randomness = [0; 32];
    sized_randomness.copy_from_slice(randomness.as_ref());

    // Randomness with the bias
    let randomness = draw_randomness(&sized_randomness, personalization, block_number, entropy);

    Ok(randomness)
}
