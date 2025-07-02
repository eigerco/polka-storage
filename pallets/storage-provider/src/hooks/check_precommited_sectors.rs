extern crate alloc;

use alloc::vec::Vec;

use frame_support::{
    pallet_prelude::{CheckedAdd, Zero},
    traits::{
        fungible::MutateHold,
        tokens::{Fortitude, Precision},
    },
};
use frame_system::pallet_prelude::BlockNumberFor;
use primitives::{sector::SectorNumber, MAX_SECTORS};
use sp_core::ConstU32;
use sp_runtime::BoundedVec;

use crate::{
    storage_provider::StorageProviderState, BalanceOf, Config, Event, HoldReason, Pallet,
    StorageProviders, LOG_TARGET,
};

/// Goes through all of the registered storage providers and checks if they have any expired pre committed sectors.
/// If there are any sectors that are expired the total deposit amount for all those sectors will be slashed.
///
/// References:
/// * <https://github.com/filecoin-project/builtin-actors/blob/82d02e58f9ef456aeaf2a6c737562ac97b22b244/actors/miner/src/state.rs#L1071>
/// * <https://github.com/filecoin-project/builtin-actors/blob/82d02e58f9ef456aeaf2a6c737562ac97b22b244/actors/miner/src/state.rs#L1054>
pub fn check_precommited_sectors<T>(current_block: BlockNumberFor<T>)
where
    T: Config,
{
    const LOG_TARGET: &'static str = "runtime::storage_provider::check_precommited_sectors";

    // TODO(@th7nder,31/07/2024): this approach is suboptimal, as it's time complexity is O(StorageProviders * PreCommitedSectors).
    // We can reduce this by indexing pre-committed sectors by BlockNumber in which they're supposed to be activated in PreCommit and remove them in ProveCommit.
    log::debug!(target: LOG_TARGET, "checking pre_commited_sectors for block: {:?}", current_block);

    // We cannot modify storage map while inside `iter_keys()` as docs say it's undefined results.
    // And we can use `alloc::Vec`, because it's bounded by StorageProviders data structure anyways.
    let storage_providers: Vec<_> = StorageProviders::<T>::iter_keys().collect();
    for storage_provider in storage_providers {
        log::debug!(target: LOG_TARGET, "checking storage provider {:?}", storage_provider);
        let Ok(mut state) = StorageProviders::<T>::try_get(storage_provider.clone()) else {
            log::error!(target: LOG_TARGET, "catastrophe, couldn't find a storage provider based on key. it should have been there...");
            continue;
        };

        let (expired, slash_amount) = detect_expired_precommit_sectors::<T>(current_block, &state);
        if expired.is_empty() {
            return;
        }

        let mut removed_sectors = BoundedVec::new();
        log::info!(target: LOG_TARGET, "found {} expired pre committed sectors for {:?}", expired.len(), storage_provider);
        for sector_number in expired {
            // Expired sectors should be removed, because in other case they'd be processed twice in the next block.
            if let Ok(()) = state.remove_pre_committed_sector(sector_number) {
                removed_sectors.force_push(sector_number)
            } else {
                log::error!(target: LOG_TARGET, "catastrophe, failed to remove sector {} for {:?}", sector_number, storage_provider);
                continue;
            };
        }

        // PRE-COND: currency was previously reserved in pre_commit
        let slash_result = T::Currency::burn_held(
            &HoldReason::ProviderPreCommitDeposit.into(),
            &storage_provider,
            slash_amount,
            Precision::BestEffort,
            Fortitude::Polite,
        );
        if slash_result != Ok(slash_amount) {
            log::error!(target: LOG_TARGET, "failed to slash.. amount: {slash_amount:?}, storage_provider: {storage_provider:?}, result: {slash_result:?}");
            continue;
        };

        StorageProviders::<T>::insert(&storage_provider, state);
        Pallet::<T>::deposit_event(Event::<T>::SectorsSlashed {
            owner: storage_provider,
            sector_numbers: removed_sectors,
        })
    }
}

/// Checks whether pre-committed sectors are expired and calculates slash amount.
///
/// THIS FUNCTION DOES NOT HANDLE ERRORS!
/// Code in hooks is assumed infallible and operates under invariants.
///
/// Returns an array of expired sector numbers and the total deposit to be slashed.
fn detect_expired_precommit_sectors<T>(
    curr_block: BlockNumberFor<T>,
    state: &StorageProviderState<T::Multiaddr, BalanceOf<T>, BlockNumberFor<T>>,
) -> (
    BoundedVec<SectorNumber, ConstU32<MAX_SECTORS>>,
    BalanceOf<T>,
)
where
    T: Config,
{
    let mut expired_sectors: BoundedVec<SectorNumber, ConstU32<MAX_SECTORS>> = BoundedVec::new();
    let mut to_be_slashed = BalanceOf::<T>::zero();

    for (sector_number, sector) in &state.pre_committed_sectors {
        // Expiration marks the time for a block when it was supposed to be proven by `prove_commit` ultimately.
        // If it's still in `pre_commited_sectors` and `curr_block` is past this time, it means it was not.
        if curr_block >= sector.info.expiration {
            let Ok(()) = expired_sectors.try_push(*sector_number) else {
                log::error!(target: LOG_TARGET, "detect_expired_precommit_sectors: invariant violated, expired_sectors bounded_vec's capacity < state.pre_committed_sectors capacity, sector: {}", sector_number);
                continue;
            };
            let Some(result) = to_be_slashed.checked_add(&sector.pre_commit_deposit) else {
                log::error!(target: LOG_TARGET, "detect_expired_precommit_sectors: invariant violated, overflow in adding slash deposit: sector: {}, current: {:?}, to add: {:?}", sector_number, to_be_slashed, sector.pre_commit_deposit);
                continue;
            };
            to_be_slashed = result;
        }
    }

    (expired_sectors, to_be_slashed)
}
