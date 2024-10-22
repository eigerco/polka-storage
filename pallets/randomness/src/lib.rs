//! # Randomness Pallet

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use frame_support::{
        dispatch::DispatchResult, pallet_prelude::*, traits::Randomness as SubstrateRandomness,
    };
    use frame_system::pallet_prelude::*;
    use pallet_insecure_randomness_collective_flip as substrate_randomness;
    use primitives_proofs::Randomness;

    pub const LOG_TARGET: &'static str = "runtime::randomness";

    #[pallet::config]
    pub trait Config: frame_system::Config + substrate_randomness::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    pub type SeedsMap<T: Config> = StorageMap<_, _, BlockNumberFor<T>, [u8; 32]>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        Randomness {
            block_number: BlockNumberFor<T>,
            randomness: [u8; 32]
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// The seed for the given block number is not available.
        SeedNotAvailable,
        /// 32 bytes are required to convert the seed into a randomness.
        ConversionError,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(_: BlockNumberFor<T>) -> Weight {
            // The determinable_after is a block number in the past since which the
            // seed is determinable by chain observers. The returned seed should
            // only be used to distinguish commitments made before the returned
            // block number.
            let (seed, determinable_after) = substrate_randomness::Pallet::<T>::random_seed();
            let seed: [u8; 32] = seed.as_ref().try_into().unwrap();

            // We are saving the seed under the determinable_after height. We
            // know that at that height the current seed was not determinable
            // and we can safely use it.
            SeedsMap::<T>::insert(determinable_after, seed);

            // TODO: Implement clearing of cached seeds. The seeds should be
            // cleared after some blocks.

            // TODO(no-ref,@cernicc,22/10/2024): Set proper weights
            T::DbWeight::get().reads(1)
        }
    }

    impl<T: Config> Pallet<T> {
        fn internal_get_randomness(block_number: BlockNumberFor<T>) -> Result<[u8; 32], DispatchError> {
            // Get the seed for the given block number
            let seed = SeedsMap::<T>::get(block_number).ok_or(Error::<T>::SeedNotAvailable)?;

            //
            let seed: [u8; 32] = seed.as_ref().try_into().map_err(|_| {
                log::error!(target: LOG_TARGET, "failed to convert randomness to [u8; 32]");
                Error::<T>::ConversionError
            })?;

            Ok(seed)
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        pub fn get_randomness(_: OriginFor<T>, block_number: BlockNumberFor<T>) -> DispatchResult {
            let seed = Self::internal_get_randomness(block_number)?;

            // TODO: Is there any other way to return the randomness
            Self::deposit_event(Event::Randomness {
                block_number,
                randomness: seed
            });

            Ok(())
        }
    }

    impl<T: Config> Randomness<BlockNumberFor<T>> for Pallet<T> {
        fn get_randomness(block_number: BlockNumberFor<T>) -> Result<[u8; 32], DispatchError> {
            Self::internal_get_randomness(block_number)

        }
    }
}
