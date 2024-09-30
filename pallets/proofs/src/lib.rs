//! # Proofs Pallet

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub(crate) use alloc::{vec, vec::Vec};

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

    use crate::{
        crypto::groth16::{self, Bls12, Proof, VerifyingKey},
        porep,
    };

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
        /// Returned when a given PoRep proof was invalid in a verification.
        InvalidPoRepProof,
        /// Returned when the given verifying key was invalid.
        InvalidVerifyingKey,
        /// Returned in case of failed conversion, i.e. in `bytes_into_fr()`.
        Conversion,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        // TODO(@neutrinoks,07.10.2024): Remove testing extrinsics (#410).
        /// Temporary! Only for testing purposes only!
        pub fn do_something(origin: OriginFor<T>, bn: u32) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            let block_number: BlockNumberFor<T> = bn.into();
            Self::deposit_event(Event::SomethingStored { block_number, who });

            Ok(().into())
        }

        // TODO(@neutrinoks,07.10.2024): Finalise interface of this extrinsic (#410).
        /// Temporary! Only for testing purposes only!
        pub fn verify_porep(
            origin: OriginFor<T>,
            seal_proof: RegisteredSealProof,
            comm_r: porep::Commitment,
            comm_d: porep::Commitment,
            sector: SectorNumber,
            ticket: porep::Ticket,
            seed: porep::Ticket,
            vkey: crate::Vec<u8>,
            proof: crate::Vec<u8>,
        ) -> DispatchResult {
            let _who = ensure_signed(origin)?;
            let vkey = VerifyingKey::<Bls12>::decode(&mut vkey.as_slice())
                .map_err(|_| Error::<T>::Conversion)?;
            let proof = Proof::<Bls12>::decode(&mut proof.as_slice())
                .map_err(|_| Error::<T>::Conversion)?;
            let proof_scheme = porep::ProofScheme::setup(seal_proof);

            // TODO(@th7nder,23/09/2024): not sure how to convert generic Account into [u8; 32]. It is AccountId32, but at this point we don't know it.
            let pvk = groth16::prepare_verifying_key(vkey);
            let account = [0u8; 32]; // TODO: who.as_ref()

            proof_scheme
                .verify(
                    &comm_r, &comm_d, &account, sector, &ticket, &seed, &pvk, &proof,
                )
                .map_err(Into::<Error<T>>::into)?;

            // TODO(@th7nder,23/09/2024): verify_porep ain't an extrinsic, this is just a method which will be called by Storage Provider Pallet via a Trait (in primitives-proofs).
            Ok(())
        }
    }
}
