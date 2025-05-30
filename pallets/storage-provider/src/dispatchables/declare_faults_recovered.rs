use frame_support::{ensure, pallet_prelude::DispatchResult};
use frame_system::{ensure_signed, pallet_prelude::OriginFor};
use sp_core::Get;

use crate::{
    deadline::DeadlineInfo, fault::DeclareFaultsRecoveredParams, sector_map::DeadlineSectorMap,
    Config, Error, Event, Pallet, StorageProviders, LOG_TARGET,
};
pub fn declare_faults_recovered<T>(
    origin: OriginFor<T>,
    params: DeclareFaultsRecoveredParams,
) -> DispatchResult
where
    T: Config,
{
    let owner = ensure_signed(origin)?;
    let current_block = <frame_system::Pallet<T>>::block_number();
    let mut sp =
        StorageProviders::<T>::try_get(&owner).map_err(|_| Error::<T>::StorageProviderNotFound)?;
    let mut to_process = DeadlineSectorMap::new();

    for term in &params.recoveries {
        let deadline = term.deadline;
        let partition = term.partition;

        // Check if the sectors passed are empty
        if term.sectors.is_empty() {
            log::error!(target: LOG_TARGET, "declare_faults_recovered: sectors cannot be empty for deadline: {:?}, partition: {:?}", deadline, partition);
            return Err(Error::<T>::GeneralPalletError(
                crate::error::GeneralPalletError::DeadlineErrorCouldNotAddSectors,
            )
            .into());
        }

        to_process
            .try_insert(deadline, partition, term.sectors.clone())
            .map_err(|e| Error::<T>::GeneralPalletError(e))?;
    }

    for (&deadline_idx, partition_map) in to_process.0.iter() {
        log::debug!(target: LOG_TARGET, "declare_faults_recovered: processing deadline index: {deadline_idx}");
        // Check deadline index to avoid doing any work if it is wrong.
        ensure!(
            (deadline_idx as usize) < sp.deadlines.due.len(),
            Error::<T>::GeneralPalletError(
                crate::error::GeneralPalletError::DeadlineErrorDeadlineIndexOutOfRange
            )
        );
        // Get the deadline
        let target_dl = DeadlineInfo::new(
            current_block,
            sp.proving_period_start,
            deadline_idx,
            T::WPoStPeriodDeadlines::get(),
            T::WPoStProvingPeriod::get(),
            T::WPoStChallengeWindow::get(),
            T::WPoStChallengeLookBack::get(),
            T::FaultDeclarationCutoff::get(),
        )
        .and_then(DeadlineInfo::next_not_elapsed)
        .map_err(|e| Error::<T>::GeneralPalletError(e))?;

        ensure!(!target_dl.fault_cutoff_passed(), {
            log::error!(target: LOG_TARGET, "declare_faults: late fault declaration at deadline {:?}. {:?} >= {:?}",
                        deadline_idx, current_block, target_dl.fault_cutoff);
            Error::<T>::FaultRecoveryTooLate
        });
        let dl = sp
            .deadlines
            .load_deadline_mut(deadline_idx as usize)
            .map_err(|e| Error::<T>::GeneralPalletError(e))?;
        dl.declare_faults_recovered(&sp.sectors, partition_map)
            .map_err(|e| Error::<T>::GeneralPalletError(e))?;
    }

    StorageProviders::<T>::insert(owner.clone(), sp);
    Pallet::<T>::deposit_event(Event::FaultsRecovered {
        owner,
        recoveries: params.recoveries,
    });

    Ok(())
}
