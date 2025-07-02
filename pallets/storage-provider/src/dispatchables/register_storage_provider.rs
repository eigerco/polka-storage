use frame_support::{dispatch::DispatchResult, ensure};
use frame_system::{
    ensure_signed,
    pallet_prelude::{BlockNumberFor, OriginFor},
};
use primitives::proofs::{assign_proving_period_offset, RegisteredPoStProof};
use sp_core::Get;

use crate::{
    storage_provider::{StorageProviderInfo, StorageProviderState},
    BalanceOf, Config, Error, Event, Pallet, StorageProviders,
};

pub fn register_storage_provider<T>(
    origin: OriginFor<T>,
    multiaddr: T::Multiaddr,
    window_post_proof_type: RegisteredPoStProof,
) -> DispatchResult
where
    T: Config,
{
    let owner = ensure_signed(origin)?;
    // Ensure that the storage provider does not exist yet
    ensure!(
        !StorageProviders::<T>::contains_key(&owner),
        Error::<T>::StorageProviderExists
    );
    let current_block = <frame_system::Pallet<T>>::block_number();
    let proving_period = T::WPoStProvingPeriod::get();

    let offset = assign_proving_period_offset::<T::AccountId, BlockNumberFor<T>>(
        &owner,
        current_block,
        T::WPoStProvingPeriod::get(),
    );

    let local_proving_start = calculate_first_proving_period_start::<BlockNumberFor<T>>(
        current_block,
        offset,
        proving_period,
    );
    let info = StorageProviderInfo::new(multiaddr, window_post_proof_type);
    let state = StorageProviderState::<T::Multiaddr, BalanceOf<T>, BlockNumberFor<T>>::new(
        info.clone(),
        local_proving_start,
        // Always zero since we're calculating the absolute first start
        // thus the deadline will always be zero
        0,
        T::WPoStPeriodDeadlines::get(),
    );
    StorageProviders::<T>::insert(&owner, state);

    // Emit event
    Pallet::<T>::deposit_event(Event::StorageProviderRegistered {
        owner,
        info,
        proving_period_start: local_proving_start,
    });
    Ok(())
}

/// Calculate the *first* proving period.
///
/// *This function deviates considerably from Filecoin.*
///
/// Since our block number (equivalent to `ChainEpoch`) is unsigned, we are not afforded the
/// luxury of calculating "current proving period" as it generates edge cases for the first
/// storage providers being registered, that is, before [`Config::WPoStChallengeWindow`] blocks
/// have elapsed).
///
/// This method will calculate the current global proving period start and add the offset to it.
/// You can read how to calculate the global proving period start and index in the description
/// for [`Config::WPoStProvingWindow`].
///
/// Reference:
/// * <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/miner/src/lib.rs#L4904-L4921>
fn calculate_first_proving_period_start<BlockNumber>(
    current_block: BlockNumber,
    offset: BlockNumber,
    wpost_proving_period: BlockNumber,
) -> BlockNumber
where
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    let global_proving_index = current_block / wpost_proving_period;
    // +1 to get the next proving period, ensuring the start is always in the future and
    // and the absolute first time the SP needs to start submitting proofs
    let global_proving_start = (global_proving_index + BlockNumber::one()) * wpost_proving_period;

    global_proving_start + offset
}

#[cfg(test)]
mod tests {
    use frame_system::pallet_prelude::BlockNumberFor;
    use rstest::rstest;

    use super::calculate_first_proving_period_start;
    use crate::tests::Test;

    // Adding +120 since it's always one full proving period ahead
    #[rstest]
    #[case(0, 0, 120)]
    #[case(0, 119, 120 + 119)]
    #[case(1, 0, 120)]
    #[case(1, 119, 120 + 119)]
    #[case(120, 0, 120 + 120)]
    #[case(120, 20, 120 + 140)]
    #[case(124, 0, 120 + 120)]
    #[case(124, 20, 120 + 140)]
    #[case(20, 5, 120 + 5)]
    fn calculate_proving_period(
        #[case] current_block: BlockNumberFor<Test>,
        #[case] offset: BlockNumberFor<Test>,
        #[case] expected_start: BlockNumberFor<Test>,
    ) {
        assert_eq!(
            calculate_first_proving_period_start::<BlockNumberFor<Test>>(
                current_block,
                offset,
                120
            ),
            expected_start
        );
    }
}
