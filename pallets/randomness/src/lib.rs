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
    };
    use frame_system::pallet_prelude::{BlockNumberFor, *};
    use sp_inherents::{InherentData, InherentIdentifier};
    use sp_runtime::traits::Hash;

    use super::GetAuthorVrf;
    use crate::inherent::{InherentError, INHERENT_IDENTIFIER};

    pub const LOG_TARGET: &'static str = "runtime::randomness";

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The Author VRF getter.
        type AuthorVrfGetter: GetAuthorVrf<Self::Hash>;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

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

            // `get_author_vrf` should only return `None` iff the BABE leader election fails
            // and falls back to the secondary slots
            //
            // References:
            // * https://github.com/paritytech/polkadot-sdk/blob/5788ae8609e1e6947c588a5745d22d8777e47f4e/substrate/frame/babe/src/lib.rs#L268-L273
            // * https://spec.polkadot.network/sect-block-production#defn-babe-secondary-slots
            if let Some(author_vrf) = T::AuthorVrfGetter::get_author_vrf() {
                AuthorVrf::<T>::put(author_vrf);
            } else {
                // We don't change the value here, this isn't great but we're not expecting
                // leader election to fail often enough that it truly affects security.
                // We're aware this is somewhat wishful thinking but time/output constraints
                // dictate that this is good enough for now!
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
}
