//! # Proofs Pallet

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub(crate) use alloc::{vec, vec::Vec};

pub use pallet::*;

mod crypto;
mod fr32;
mod graphs;
mod porep;
mod post;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    pub const LOG_TARGET: &'static str = "runtime::proofs";

    use frame_support::{pallet_prelude::*, sp_runtime::BoundedBTreeMap};
    use frame_system::pallet_prelude::*;
    use primitives::proofs::{
        ProofVerification, ProverId, PublicReplicaInfo, RawCommitment, RegisteredPoStProof,
        RegisteredSealProof, SectorNumber, Ticket, MAX_POST_PROOF_BYTES, MAX_SEAL_PROOF_BYTES,
        MAX_SECTORS_PER_PROOF,
    };

    use crate::{
        crypto::groth16::{Bls12, Proof, VerifyingKey},
        porep, post,
    };

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    /// Verifying Key for verifying all of the PoRep proofs generated for 2KiB sectors.
    /// One per runtime.
    ///
    /// It should be set via some kind of trusted setup procedure.
    /// /// Test key can be generated via `polka-storage-provider-client utils porep-params`.
    /// To support more sector sizes for proofs, this data structure would need to be a Map from Sector Size to a Verifying Key.
    #[pallet::storage]
    pub type PoRepVerifyingKey<T: Config> = StorageValue<_, VerifyingKey<Bls12>, OptionQuery>;

    /// Verifying Key for verifying all of the PoSt proofs generated for 2KiB sectors.
    /// One per runtime.
    ///
    /// It should be set via some kind of trusted setup procedure.
    /// Test key can be generated via `polka-storage-provider-client utils post-params`.
    /// To support more sector sizes for proofs, this data structure would need to be a Map from Sector Size to a Verifying Key.
    #[pallet::storage]
    pub type PoStVerifyingKey<T: Config> = StorageValue<_, VerifyingKey<Bls12>, OptionQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        PoRepVerifyingKeyChanged { who: T::AccountId },
        PoStVerifyingKeyChanged { who: T::AccountId },
    }

    #[pallet::error]
    pub enum Error<T> {
        InvalidPoStProof,
        MissingPoStVerifyingKey,
        MissingPoRepVerifyingKey,
        InvalidPoRepProof,
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
            let vkey =
                VerifyingKey::<Bls12>::decode(&mut verifying_key.as_slice()).map_err(|e| {
                    log::error!(target: LOG_TARGET, "failed to parse PoRep verifying key {:?}", e);
                    Error::<T>::Conversion
                })?;

            PoRepVerifyingKey::<T>::set(Some(vkey));

            Self::deposit_event(Event::PoRepVerifyingKeyChanged { who: caller });

            Ok(())
        }

        pub fn set_post_verifying_key(
            origin: OriginFor<T>,
            verifying_key: crate::Vec<u8>,
        ) -> DispatchResult {
            let caller = ensure_signed(origin)?;
            let vkey =
                VerifyingKey::<Bls12>::decode(&mut verifying_key.as_slice()).map_err(|e| {
                    log::error!(target: LOG_TARGET, "failed to parse PoSt verifying key {:?}", e);
                    Error::<T>::Conversion
                })?;

            PoStVerifyingKey::<T>::set(Some(vkey));

            Self::deposit_event(Event::PoStVerifyingKeyChanged { who: caller });

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
            proof: BoundedVec<u8, ConstU32<MAX_SEAL_PROOF_BYTES>>,
        ) -> DispatchResult {
            let proof_len = proof.len();
            ensure!(proof_len >= seal_proof.proof_size(), {
                log::error!(
                    target: LOG_TARGET,
                    "PoRep proof submission does not contain enough bytes. Expected minimum length is {} got {}",
                    seal_proof.proof_size(), proof_len
                );
                Error::<T>::InvalidPoRepProof
            });
            let proof = Proof::<Bls12>::decode(&mut proof.as_slice()).map_err(|e| {
                log::error!(target: LOG_TARGET, "failed to parse PoRep proof {:?}", e);
                Error::<T>::Conversion
            })?;
            let proof_scheme = porep::ProofScheme::setup(seal_proof);

            let vkey = PoRepVerifyingKey::<T>::get().ok_or(Error::<T>::MissingPoRepVerifyingKey)?;
            log::info!(target: LOG_TARGET, "Verifying PoRep proof for sector: {}...", sector);
            proof_scheme
                .verify(
                    &comm_r, &comm_d, &prover_id, sector, &ticket, &seed, vkey, &proof,
                )
                .map_err(Into::<Error<T>>::into)?;

            Ok(())
        }

        fn verify_post(
            post_type: RegisteredPoStProof,
            randomness: Ticket,
            replicas: BoundedBTreeMap<
                SectorNumber,
                PublicReplicaInfo,
                ConstU32<MAX_SECTORS_PER_PROOF>,
            >,
            proof: BoundedVec<u8, ConstU32<MAX_POST_PROOF_BYTES>>,
        ) -> DispatchResult {
            let replica_count = replicas.len();
            ensure!(replica_count <= post_type.sector_count(), {
                log::error!(
                    target: LOG_TARGET,
                    "Got more replicas than expected. Expected max replicas = {}, submitted replicas = {replica_count}",
                    post_type.sector_count()
                );
                Error::<T>::InvalidPoStProof
            });
            let proof = Proof::<Bls12>::decode(&mut proof.as_slice()).map_err(|e| {
                log::error!(target: LOG_TARGET, "failed to parse PoSt proof {:?}", e);
                Error::<T>::Conversion
            })?;
            let proof_scheme = post::ProofScheme::setup(post_type);

            let vkey = PoStVerifyingKey::<T>::get().ok_or(Error::<T>::MissingPoStVerifyingKey)?;
            proof_scheme
                .verify(randomness, replicas.clone(), vkey, proof)
                .map_err(|e| {
                    log::warn!(target: LOG_TARGET, "failed to verify PoSt proof: {:?}, for replicas: {:?}", e, replicas);
                    Error::<T>::InvalidPoStProof
                })?;

            Ok(())
        }
    }
}
