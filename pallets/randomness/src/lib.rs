//! # Randomness Pallet

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

mod types;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    extern crate alloc;

    use alloc::vec::Vec;

    use frame_support::{
        dispatch::DispatchResult,
        pallet_prelude::{ValueQuery, *},
        traits::Randomness as SubstrateRandomness,
        Twox64Concat,
    };
    use frame_system::pallet_prelude::{OriginFor, *};
    use primitives::pallets::Randomness;
    use sp_runtime::{traits::Zero, Saturating};

    use crate::types::{RandomnessResult, RequestType};

    pub const LOG_TARGET: &'static str = "runtime::randomness";

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Underlying randomness generator
        type Generator: SubstrateRandomness<Self::Hash, BlockNumberFor<Self>>;

        /// Clean-up interval specified in number of blocks between cleanups.
        #[pallet::constant]
        type CleanupInterval: Get<BlockNumberFor<Self>>;

        /// The number of blocks after which the seed is cleaned up.
        #[pallet::constant]
        type SeedAgeLimit: Get<BlockNumberFor<Self>>;

        /// Get BABE data from the runtime.
        type BabeDataGetter: GetBabeData<u64, Option<Self::Hash>>;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn seeds)]
    pub type SeedsMap<T: Config> = StorageMap<_, _, BlockNumberFor<T>, [u8; 32]>;

    #[pallet::storage]
    #[pallet::getter(fn randomness_results)]
    pub type RandomnessResults<T: Config> =
        StorageMap<_, Twox64Concat, RequestType<T>, RandomnessResult<T::Hash>>;

    #[pallet::storage]
    #[pallet::getter(fn relay_epoch)]
    pub(crate) type RelayEpoch<T: Config> = StorageValue<_, u64, ValueQuery>;

    #[pallet::error]
    pub enum Error<T> {
        /// The seed for the given block number is not available.
        SeedNotAvailable,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        pub fn set_babe_randomness_results(origin: OriginFor<T>) -> DispatchResult {
            ensure_none(origin)?;
            let last_relay_epoch_index = <RelayEpoch<T>>::get();
            let relay_epoch_index = T::BabeDataGetter::get_epoch_index();
            if relay_epoch_index > last_relay_epoch_index {
                let babe_one_epoch_ago_this_block = RequestType::BabeEpoch(relay_epoch_index);
                if let Some(mut results) =
                    <RandomnessResults<T>>::get(&babe_one_epoch_ago_this_block)
                {
                    if let Some(randomness) = T::BabeDataGetter::get_epoch_randomness() {
                        results.randomness = Some(randomness);
                        <RandomnessResults<T>>::insert(babe_one_epoch_ago_this_block, results);
                    } else {
                        log::warn!(
                            "Failed to fill BABE epoch randomness results \
                            REQUIRE HOTFIX TO FILL EPOCH RANDOMNESS RESULTS FOR EPOCH {:?}",
                            relay_epoch_index
                        );
                    }
                }
            }
            <RelayEpoch<T>>::put(relay_epoch_index);
            Ok(())
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(block_number: BlockNumberFor<T>) -> Weight {
            // TODO(no-ref,@cernicc,22/10/2024): Set proper weights
            let weight = T::DbWeight::get().reads(1);

            // The determinable_after is a block number in the past since which
            // the current seed is determinable by chain observers.
            let (seed, determinable_after) = T::Generator::random_seed();
            let seed: [u8; 32] = seed.as_ref().try_into().expect("seed should be 32 bytes");

            // We are not saving the seed for the zeroth block. This is an edge
            // case when trying to use randomness at the network genesis.
            if determinable_after == Zero::zero() {
                return weight;
            }

            // Save the seed
            SeedsMap::<T>::insert(block_number, seed);
            log::info!(target: LOG_TARGET, "on_initialize: height: {block_number:?}, seed: {seed:?}");

            weight
        }

        fn on_finalize(current_block_number: BlockNumberFor<T>) {
            // Check if we should clean the seeds
            if current_block_number % T::CleanupInterval::get() != Zero::zero() {
                return;
            }

            // Mark which seeds to remove
            let mut blocks_to_remove = Vec::new();
            for creation_height in SeedsMap::<T>::iter_keys() {
                let age_limit = T::SeedAgeLimit::get();
                let current_age = current_block_number.saturating_sub(creation_height);

                // Seed is old enough to be removed
                if current_age >= age_limit {
                    blocks_to_remove.push(creation_height);
                }
            }

            // Remove old seeds
            blocks_to_remove.iter().for_each(|number| {
                SeedsMap::<T>::remove(number);
            });
        }
    }

    impl<T: Config> Randomness<BlockNumberFor<T>> for Pallet<T> {
        fn get_randomness(block_number: BlockNumberFor<T>) -> Result<[u8; 32], DispatchError> {
            // Get the seed for the given block number
            let current_block_number = frame_system::Pallet::<T>::block_number();
            let seed = SeedsMap::<T>::get(block_number).ok_or_else(|| {
                log::error!(target: LOG_TARGET, "get_randomness: No seed available for {block_number:?} at {current_block_number:?}");
                Error::<T>::SeedNotAvailable
            })?;

            Ok(seed)
        }
    }

    pub trait GetBabeData<EpochIndex, Randomness> {
        fn get_epoch_index() -> EpochIndex;
        fn get_epoch_randomness() -> Randomness;
    }
}
