extern crate alloc;

use alloc::vec::Vec;

use frame_support::{dispatch::DispatchResult, ensure};
use frame_system::{ensure_signed, pallet_prelude::OriginFor};
use sp_core::Get;

use super::process_early_terminations;
use crate::{
    deadline::deadline_is_mutable, sector::TerminateSectorsParams, sector_map::DeadlineSectorMap,
    Config, Error, Event, Pallet, StorageProviders, LOG_TARGET,
};

pub fn terminate_sectors<T>(origin: OriginFor<T>, params: TerminateSectorsParams) -> DispatchResult
where
    T: Config,
{
    let owner = ensure_signed(origin)?;
    let current_block = <frame_system::Pallet<T>>::block_number();
    let mut sp =
        StorageProviders::<T>::try_get(&owner).map_err(|_| Error::<T>::StorageProviderNotFound)?;

    let mut to_process = DeadlineSectorMap::new();

    for term in params.terminations.iter() {
        let deadline = term.deadline;
        let partition = term.partition;

        to_process
            .try_insert(deadline, partition, term.sectors.clone())
            .map_err(|e| Error::<T>::GeneralPalletError(e))?;
    }

    let sectors = sp
        .sectors
        .iter()
        .map(|(_sector_number, info)| info)
        .cloned()
        .collect::<Vec<_>>();
    for (&deadline_idx, partition_sectors) in to_process.into_iter() {
        ensure!(
            deadline_is_mutable(
                sp.proving_period_start,
                deadline_idx,
                current_block,
                T::WPoStPeriodDeadlines::get(),
                T::WPoStProvingPeriod::get(),
                T::WPoStChallengeWindow::get(),
                T::WPoStChallengeLookBack::get(),
                T::FaultDeclarationCutoff::get(),
            )
            .map_err(|e| Error::<T>::GeneralPalletError(e))?,
            {
                log::error!(target: LOG_TARGET, "cannot terminate sectors in immutable deadline {}", deadline_idx);
                Error::<T>::CannotTerminateImmutableDeadline
            }
        );

        let deadline = sp
            .deadlines
            .load_deadline_mut(deadline_idx as usize)
            .map_err(|e| Error::<T>::GeneralPalletError(e))?;

        deadline
            .terminate_sectors(current_block, &sectors, partition_sectors)
            .map_err(|e| Error::<T>::GeneralPalletError(e))?;

        sp.early_terminations.insert(deadline_idx);
    }

    // Update storage provider state
    StorageProviders::<T>::insert(&owner, sp);

    process_early_terminations::<T>(current_block, &owner)?;

    Pallet::<T>::deposit_event(Event::SectorsTerminated {
        owner,
        terminations: params.terminations,
    });
    Ok(())
}
