//! # Proofs Pallet
//!

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*};
    use frame_system::pallet_prelude::*;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        SomethingStored {
            block_number: BlockNumberFor<T>,
            who: T::AccountId,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        NoneValue,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        pub fn do_something(origin: OriginFor<T>, bn: u32) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            let block_number: BlockNumberFor<T> = bn.into();
            Self::deposit_event(Event::SomethingStored { block_number, who });

            Ok(().into())
        }
    }
}
