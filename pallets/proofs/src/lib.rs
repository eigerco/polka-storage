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
pub mod weights;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[frame_support::pallet]
pub mod pallet {
    pub const LOG_TARGET: &'static str = "runtime::proofs";

    use frame_support::{pallet_prelude::*, sp_runtime::BoundedBTreeMap};
    use frame_system::pallet_prelude::*;
    use primitives::{
        commitment::RawCommitment,
        pallets::ProofVerification,
        proofs::{ProverId, PublicReplicaInfo, RegisteredPoStProof, RegisteredSealProof, Ticket},
        sector::SectorNumber,
        MAX_POST_PROOF_BYTES, MAX_PROOFS_PER_BLOCK, MAX_REPLICAS_PER_BLOCK, MAX_SEAL_PROOF_BYTES,
    };

    use crate::{
        crypto::groth16::{Bls12, Proof, VerifyingKey},
        porep, post,
        weights::WeightInfo,
    };

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type WeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    /// [`VerifyingKey`]s for Proofs of Replication.
    #[pallet::storage]
    pub type PoRepVerifyingKeys<T: Config> =
        StorageMap<_, Blake2_128Concat, RegisteredSealProof, VerifyingKey<Bls12>>;

    /// [`VerifyingKey`]s for Proofs of Spacetime.
    #[pallet::storage]
    pub type PoStVerifyingKeys<T: Config> =
        StorageMap<_, Blake2_128Concat, RegisteredPoStProof, VerifyingKey<Bls12>>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        PoRepVerifyingKeyChanged {
            who: T::AccountId,
            proof: RegisteredSealProof,
        },
        PoStVerifyingKeyChanged {
            who: T::AccountId,
            proof: RegisteredPoStProof,
        },
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
        #[pallet::call_index(0)]
        #[pallet::weight((T::WeightInfo::set_porep_verifying_key(), DispatchClass::Operational))]
        pub fn set_porep_verifying_key(
            origin: OriginFor<T>,
            registered_seal_proof: RegisteredSealProof,
            verifying_key: crate::Vec<u8>,
        ) -> DispatchResult {
            let caller = ensure_signed(origin)?;
            let vkey =
                VerifyingKey::<Bls12>::decode(&mut verifying_key.as_slice()).map_err(|e| {
                    log::error!(target: LOG_TARGET, "failed to parse PoRep verifying key {:?}", e);
                    Error::<T>::Conversion
                })?;

            PoRepVerifyingKeys::<T>::insert(registered_seal_proof, vkey);
            Self::deposit_event(Event::PoRepVerifyingKeyChanged {
                who: caller,
                proof: registered_seal_proof,
            });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight((T::WeightInfo::set_post_verifying_key(), DispatchClass::Operational))]
        pub fn set_post_verifying_key(
            origin: OriginFor<T>,
            registered_post_proof: RegisteredPoStProof,
            verifying_key: crate::Vec<u8>,
        ) -> DispatchResult {
            let caller = ensure_signed(origin)?;
            let vkey =
                VerifyingKey::<Bls12>::decode(&mut verifying_key.as_slice()).map_err(|e| {
                    log::error!(target: LOG_TARGET, "failed to parse PoSt verifying key {:?}", e);
                    Error::<T>::Conversion
                })?;

            PoStVerifyingKeys::<T>::insert(registered_post_proof, vkey);
            Self::deposit_event(Event::PoStVerifyingKeyChanged {
                who: caller,
                proof: registered_post_proof,
            });
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
            proofs: BoundedVec<
                BoundedVec<u8, ConstU32<MAX_SEAL_PROOF_BYTES>>,
                ConstU32<MAX_PROOFS_PER_BLOCK>,
            >,
        ) -> DispatchResult {
            let mut parsed_proofs = BoundedVec::new();
            for proof in proofs.iter() {
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

                parsed_proofs.try_push(proof).expect("internal (porep::ProofScheme) and external (ProofVerification) apis have the same limits on number of proofs");
            }
            let proof_scheme = porep::ProofScheme::setup(seal_proof);

            let vkey = PoRepVerifyingKeys::<T>::get(seal_proof)
                .ok_or(Error::<T>::MissingPoRepVerifyingKey)?;
            log::info!(target: LOG_TARGET, "Verifying PoRep proof for sector: {}...", sector);
            proof_scheme
                .verify(
                    &comm_r,
                    &comm_d,
                    &prover_id,
                    sector,
                    &ticket,
                    &seed,
                    vkey,
                    parsed_proofs,
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
                ConstU32<MAX_REPLICAS_PER_BLOCK>,
            >,
            proofs: BoundedVec<
                BoundedVec<u8, ConstU32<MAX_POST_PROOF_BYTES>>,
                ConstU32<MAX_PROOFS_PER_BLOCK>,
            >,
        ) -> DispatchResult {
            let replica_count = replicas.len();
            ensure!(replica_count <= post_type.sector_count() * proofs.len(), {
                log::error!(
                    target: LOG_TARGET,
                    "Got more replicas than expected. Expected max replicas = {}, submitted replicas = {replica_count}",
                    post_type.sector_count()
                );
                Error::<T>::InvalidPoStProof
            });
            let mut parsed_proofs = BoundedVec::new();
            for (index, proof) in proofs.into_iter().enumerate() {
                let proof = Proof::<Bls12>::decode(&mut proof.as_slice()).map_err(|e| {
                    log::error!(target: LOG_TARGET, "failed to parse PoSt proof (idx: {}){:?}", index, e);
                    Error::<T>::Conversion
                })?;

                parsed_proofs.try_push(proof).expect(
                    "internal (post::ProofScheme) and external (ProofVerification) apis have the same limits on number of proofs",
                );
            }

            let proof_scheme = post::ProofScheme::setup(post_type);

            let vkey = PoStVerifyingKeys::<T>::get(post_type)
                .ok_or(Error::<T>::MissingPoStVerifyingKey)?;
            proof_scheme
                .verify(randomness, replicas.clone(), vkey, parsed_proofs)
                .map_err(|e| {
                    log::warn!(target: LOG_TARGET, "failed to verify PoSt proof: {:?}, for replicas: {:?}", e, replicas);
                    Error::<T>::InvalidPoStProof
                })?;

            Ok(())
        }
    }
}
