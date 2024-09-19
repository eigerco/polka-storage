//! # Proofs Pallet
//!

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

mod crypto;
mod fr32;
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
    use primitives_proofs::{RegisteredSealProof, SectorNumber};

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
        InvalidProof,
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
            origin: OriginFor<T>,
            seal_proof: RegisteredSealProof,
            comm_r: porep::Commitment,
            comm_d: porep::Commitment,
            sector: SectorNumber,
            ticket: porep::Ticket,
            seed: porep::Ticket,
        ) -> DispatchResultWithPostInfo {
            let _who = ensure_signed(origin)?;
            let proof_scheme = porep::ProofScheme::setup(seal_proof);

            // TODO(@th7nder,23/09/2024): not sure how to convert generic Account into [u8; 32]. It is AccountId32, but at this point we don't know it.
            let account = [0u8; 32];

            let _result = proof_scheme
                .verify(&comm_r, &comm_d, &account, sector, &ticket, &seed)
                .map_err(|_| Error::<T>::InvalidProof)?;

            // TODO(@th7nder,23/09/2024): verify_porep ain't an extrinsic, this is just a method which will be called by Storage Provider Pallet via a Trait (in primitives-proofs).
            Ok(().into())
        }
    }
}
