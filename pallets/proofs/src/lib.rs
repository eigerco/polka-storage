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
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use primitives_proofs::{
        ProofVerification, ProverId, RawCommitment, RegisteredSealProof, SectorNumber, Ticket,
    };

    use crate::{
        crypto::groth16::{Bls12, Proof, VerifyingKey},
        porep,
    };

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    pub type PoRepVerifyingKey<T: Config> = StorageValue<_, VerifyingKey<Bls12>, OptionQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        PoRepVerifyingKeyChanged { who: T::AccountId },
    }

    #[pallet::error]
    pub enum Error<T> {
        MissingPoRepVerifyingKey,
        /// Returned when a given PoRep proof was invalid in a verification.
        InvalidPoRepProof,
        /// Returned when the given verifying key was invalid.
        InvalidVerifyingKey,
        /// Returned in case of failed conversion, i.e. in `bytes_into_fr()`.
        Conversion,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        pub fn set_porep_verifying_key(
            origin: OriginFor<T>,
            verifying_key: crate::Vec<u8>,
        ) -> DispatchResult {
            let caller = ensure_signed(origin)?;
            let vkey = VerifyingKey::<Bls12>::decode(&mut verifying_key.as_slice())
                .map_err(|_| Error::<T>::Conversion)?;

            PoRepVerifyingKey::<T>::set(Some(vkey));

            Self::deposit_event(Event::PoRepVerifyingKeyChanged { who: caller });

            Ok(())
        }
    }

    impl<T: Config> ProofVerification for Pallet<T> {
        fn verify_porep(
            prover_id: ProverId,
            seal_proof: RegisteredSealProof,
            comm_r: RawCommitment,
            comm_d: RawCommitment,
            sector: SectorNumber,
            ticket: Ticket,
            seed: Ticket,
            proof: crate::Vec<u8>,
        ) -> DispatchResult {
            let proof = Proof::<Bls12>::decode(&mut proof.as_slice())
                .map_err(|_| Error::<T>::Conversion)?;
            let proof_scheme = porep::ProofScheme::setup(seal_proof);

            let vkey = PoRepVerifyingKey::<T>::get().ok_or(Error::<T>::MissingPoRepVerifyingKey)?;
            proof_scheme
                .verify(
                    &comm_r, &comm_d, &prover_id, sector, &ticket, &seed, vkey, &proof,
                )
                .map_err(Into::<Error<T>>::into)?;

            Ok(())
        }
    }
}
