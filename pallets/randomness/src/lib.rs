//! # Randomness Pallet

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

    use primitives_proofs::{Randomness};

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    pub type SeedsMap<T: Config> =
        StorageMap<_, _, BlockNumberFor<T>, [u8; 32]>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        Test { who: T::AccountId },
    }

    #[pallet::error]
    pub enum Error<T> {}

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        pub fn get_randomness(
            origin: OriginFor<T>,
        ) -> DispatchResult {
            Ok(())
        }
    }

    impl<T: Config> Randomness for Pallet<T> {
        fn get_randomness() -> DispatchResult {
            Ok(())
        }
    }
}
