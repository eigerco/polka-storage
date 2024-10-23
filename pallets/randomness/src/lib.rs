//! # Randomness Pallet

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use frame_support::{pallet_prelude::*, traits::Randomness as SubstrateRandomness};
    use frame_system::pallet_prelude::*;
    use pallet_insecure_randomness_collective_flip as substrate_randomness;
    use primitives_proofs::Randomness;
    use sp_runtime::traits::Zero;

    pub const LOG_TARGET: &'static str = "runtime::randomness";

    #[pallet::config]
    pub trait Config: frame_system::Config + substrate_randomness::Config {}

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
        fn on_initialize(_: BlockNumberFor<T>) -> Weight {
            // TODO(no-ref,@cernicc,22/10/2024): Set proper weights
            let weight = T::DbWeight::get().reads(1);

            // The determinable_after is a block number in the past since which
            // the current seed is determinable by chain observers. The returned
            // seed should only be used to distinguish commitments made before
            // the returned determinable_after.
            let (seed, determinable_after) = substrate_randomness::Pallet::<T>::random_seed();
            let seed: [u8; 32] = seed.as_ref().try_into().expect("seed should be 32 bytes");

            // We are not saving the seed for the zeroth block. This is an edge
            // case when trying to use randomness at the network genesis.
            if determinable_after == Zero::zero() {
                return weight;
            }

            // We are saving the seed under the determinable_after height. We
            // know that at that height the current seed was not determinable
            // and we can safely use it.
            SeedsMap::<T>::insert(determinable_after, seed);

            // TODO(no-ref,@cernicc,23/10/2024): Should we remove seeds that are
            // older then some specified number of blocks?

            weight
        }
    }

    impl<T: Config> Randomness<BlockNumberFor<T>> for Pallet<T> {
        fn get_randomness(block_number: BlockNumberFor<T>) -> Result<[u8; 32], DispatchError> {
            // Get the seed for the given block number
            let seed = SeedsMap::<T>::get(block_number).ok_or(Error::<T>::SeedNotAvailable)?;
            Ok(seed)
        }
    }
}
