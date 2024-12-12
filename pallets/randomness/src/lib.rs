//! # Randomness Pallet

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

mod inherent;

pub trait GetAuthorVrf<H>
where
    H: core::hash::Hash,
{
    fn get_author_vrf() -> Option<H>;
}

#[frame_support::pallet(dev_mode)]
pub mod pallet {

    extern crate alloc;

    use alloc::vec::Vec;

    use frame_support::{
        inherent::ProvideInherent,
        pallet_prelude::{ValueQuery, *},
        traits::Randomness as SubstrateRandomness,
    };
    use frame_system::pallet_prelude::{BlockNumberFor, *};
    use sp_inherents::{InherentData, InherentIdentifier};
    use sp_runtime::traits::Hash;

    use super::GetAuthorVrf;
    use crate::inherent::{InherentError, INHERENT_IDENTIFIER};

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

        type AuthorVrfGetter: GetAuthorVrf<Self::Hash>;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn seeds)]
    pub type SeedsMap<T: Config> = StorageMap<_, _, BlockNumberFor<T>, [u8; 32]>;

    #[pallet::error]
    pub enum Error<T> {
        /// The seed for the given block number is not available.
        SeedNotAvailable,
    }

    #[pallet::storage]
    #[pallet::getter(fn author_vrf)]
    pub type AuthorVrf<T: Config> = StorageValue<_, T::Hash, ValueQuery>;

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        pub fn set_author_vrf(origin: OriginFor<T>) -> DispatchResult {
            ensure_none(origin)?;

            if let Some(author_vrf) = T::AuthorVrfGetter::get_author_vrf() {
                AuthorVrf::<T>::put(author_vrf);
            } else {
                log::warn!("AuthorVrf is empty, keeping previous value");
            }

            Ok(())
        }
    }

    #[pallet::inherent]
    impl<T: Config> ProvideInherent for Pallet<T> {
        type Call = Call<T>;
        type Error = InherentError;

        const INHERENT_IDENTIFIER: InherentIdentifier = INHERENT_IDENTIFIER;

        fn is_inherent_required(_: &InherentData) -> Result<Option<Self::Error>, Self::Error> {
            // Return Ok(Some(_)) unconditionally because this inherent is required in every block
            // If it is not found, throw a VrfInherentRequired error.
            Ok(Some(InherentError::Other(
                sp_runtime::RuntimeString::Borrowed(
                    "Inherent required to set babe randomness results",
                ),
            )))
        }

        // The empty-payload inherent extrinsic.
        fn create_inherent(_data: &InherentData) -> Option<Self::Call> {
            Some(Call::set_author_vrf {})
        }

        fn is_inherent(call: &Self::Call) -> bool {
            matches!(call, Call::set_author_vrf { .. })
        }
    }

    impl<T: Config> frame_support::traits::Randomness<T::Hash, BlockNumberFor<T>> for Pallet<T> {
        fn random(subject: &[u8]) -> (T::Hash, BlockNumberFor<T>) {
            let author_vrf = AuthorVrf::<T>::get();
            let block_number = frame_system::Pallet::<T>::block_number();
            let mut digest = Vec::new();
            digest.extend_from_slice(author_vrf.as_ref());
            digest.extend_from_slice(subject);
            let randomness = T::Hashing::hash(digest.as_slice());
            (randomness, block_number)
        }
    }

    // #[pallet::hooks]
    // impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
    //     fn on_initialize(block_number: BlockNumberFor<T>) -> Weight {
    //         // TODO(no-ref,@cernicc,22/10/2024): Set proper weights
    //         let weight = T::DbWeight::get().reads(1);

    //         // The determinable_after is a block number in the past since which
    //         // the current seed is determinable by chain observers.
    //         let (seed, determinable_after) = T::Generator::random_seed();
    //         let seed: [u8; 32] = seed.as_ref().try_into().expect("seed should be 32 bytes");

    //         // We are not saving the seed for the zeroth block. This is an edge
    //         // case when trying to use randomness at the network genesis.
    //         if determinable_after == Zero::zero() {
    //             return weight;
    //         }

    //         // Save the seed
    //         SeedsMap::<T>::insert(block_number, seed);
    //         log::info!(target: LOG_TARGET, "on_initialize: height: {block_number:?}, seed: {seed:?}");

    //         weight
    //     }

    //     fn on_finalize(current_block_number: BlockNumberFor<T>) {
    //         // Check if we should clean the seeds
    //         if current_block_number % T::CleanupInterval::get() != Zero::zero() {
    //             return;
    //         }

    //         // Mark which seeds to remove
    //         let mut blocks_to_remove = Vec::new();
    //         for creation_height in SeedsMap::<T>::iter_keys() {
    //             let age_limit = T::SeedAgeLimit::get();
    //             let current_age = current_block_number.saturating_sub(creation_height);

    //             // Seed is old enough to be removed
    //             if current_age >= age_limit {
    //                 blocks_to_remove.push(creation_height);
    //             }
    //         }

    //         // Remove old seeds
    //         blocks_to_remove.iter().for_each(|number| {
    //             SeedsMap::<T>::remove(number);
    //         });
    //     }
    // }
}
