use frame_support::{dispatch::DispatchResult, ensure};
use frame_system::{ensure_signed, pallet_prelude::OriginFor};
use sp_core::Get;

use crate::{
    deadline::DeadlineInfo, fault::DeclareFaultsParams, sector_map::DeadlineSectorMap, Config,
    Error, Event, Pallet, StorageProviders, LOG_TARGET,
};

pub fn declare_faults<T>(origin: OriginFor<T>, params: DeclareFaultsParams) -> DispatchResult
where
    T: Config,
{
    let owner = ensure_signed(origin)?;
    let current_block = <frame_system::Pallet<T>>::block_number();
    let mut sp =
        StorageProviders::<T>::try_get(&owner).map_err(|_| Error::<T>::StorageProviderNotFound)?;

    let mut to_process = DeadlineSectorMap::new();
    for term in &params.faults {
        let deadline = term.deadline;
        let partition = term.partition;

        // Check if the sectors passed are empty
        if term.sectors.is_empty() {
            log::error!(target: LOG_TARGET, "declare_faults: [deadline: {}, partition: {}] cannot add empty sectors", deadline, partition);
            return Err(Error::<T>::GeneralPalletError(
                crate::error::GeneralPalletError::DeadlineErrorCouldNotAddSectors,
            )
            .into());
        }

        to_process
            .try_insert(deadline, partition, term.sectors.clone())
            .map_err(|e| Error::<T>::GeneralPalletError(e))?;
    }

    for (&deadline_idx, partition_map) in to_process.into_iter() {
        log::debug!(target: LOG_TARGET, "declare_faults: Processing deadline index: {deadline_idx}");
        // Check deadline index to avoid doing any work if it is wrong.
        ensure!(
            (deadline_idx as usize) < sp.deadlines.due.len(),
            Error::<T>::GeneralPalletError(
                crate::error::GeneralPalletError::DeadlineErrorDeadlineIndexOutOfRange
            )
        );
        // Get the target deadline
        // We're deviating from the original implementation by using the `sp.proving_period_start`
        // instead of calculating it here, but we couldn't find a reason to do it in another way
        //
        // References:
        // * https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/miner/src/lib.rs#L2436-L2449
        // * https://github.com/eigerco/polka-storage/pull/192#discussion_r1715067288
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

        // https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/miner/src/lib.rs#L2451-L2458
        ensure!(!target_dl.fault_cutoff_passed(), {
            log::error!(target: LOG_TARGET, "declare_faults: Late fault declaration at deadline {:?}. {:?} >= {:?}", deadline_idx, current_block, target_dl.fault_cutoff);
            Error::<T>::FaultDeclarationTooLate
        });

        let fault_expiration_block = target_dl.last() + T::FaultMaxAge::get();
        log::debug!(target: LOG_TARGET, "declare_faults: Getting deadline[{deadline_idx}]");
        let dl = sp
            .deadlines
            .load_deadline_mut(deadline_idx as usize)
            .map_err(|e| Error::<T>::GeneralPalletError(e))?;

        dl.record_faults(&sp.sectors, partition_map, fault_expiration_block)
            .map_err(|e| Error::<T>::GeneralPalletError(e))?;
    }

    StorageProviders::<T>::set(owner.clone(), Some(sp));
    Pallet::<T>::deposit_event(Event::FaultsDeclared {
        owner,
        faults: params.faults,
    });

    Ok(())
}
