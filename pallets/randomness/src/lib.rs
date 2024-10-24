//! # Randomness Pallet

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    extern crate alloc;

    use alloc::vec::Vec;

    use frame_support::{pallet_prelude::*, traits::Randomness as SubstrateRandomness};
    use frame_system::pallet_prelude::*;
    use primitives_proofs::Randomness;
    use sp_runtime::{traits::Zero, Saturating};

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
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    pub type SeedsMap<T: Config> = StorageMap<_, _, BlockNumberFor<T>, [u8; 32]>;

    #[pallet::error]
    pub enum Error<T> {
        /// The seed for the given block number is not available.
        SeedNotAvailable,
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
}
