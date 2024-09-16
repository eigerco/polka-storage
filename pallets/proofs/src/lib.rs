//! # Proofs Pallet
//!

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

mod graphs;
mod porep;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*};
    use frame_system::pallet_prelude::*;
    use primitives_proofs::RegisteredSealProof;

    use crate::porep;

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

        pub fn verify_porep(
            _origin: OriginFor<T>,
            seal_proof: RegisteredSealProof,
        ) -> DispatchResultWithPostInfo {
            let proof_scheme = porep::ProofScheme::setup(seal_proof);

            proof_scheme.verify();

            Ok(().into())
        }
    }
}
