//! # Storage Provider Pallet
//!
//! This pallet is responsible for:
//! - Storage proving operations
//! - Used by the storage provider to generate and submit Proof-of-Replication (PoRep) and Proof-of-Spacetime (PoSt).
//! - Managing and handling collateral for storage deals, penalties, and rewards related to storage deal performance.
//!
//! This pallet holds information about storage providers and provides an interface to modify that information.
//!
//! The Storage Provider Pallet is the source of truth for anything storage provider related.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;

mod types;

pub use pallet::{Config, Pallet};

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use crate::types::{
        PoStProof, RegisteredPoStProof, SectorNumber, SectorPreCommitInfo, StorageProviderInfo,
    };

    use codec::{Decode, Encode};
    use core::fmt::Debug;
    use frame_support::dispatch::DispatchResultWithPostInfo;
    use frame_support::ensure;
    use frame_support::pallet_prelude::{IsType, StorageMap};
    use frame_system::ensure_signed;
    use frame_system::pallet_prelude::OriginFor;
    use scale_info::prelude::vec::Vec;
    use scale_info::TypeInfo;

    #[pallet::pallet]
    #[pallet::without_storage_info] // Allows to define storage items without fixed size
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Peer ID is derived by hashing an encoded public key.
        /// Usually represented in bytes.
        /// https://github.com/libp2p/specs/blob/master/peer-ids/peer-ids.md#peer-ids
        type PeerId: Clone + Debug + Decode + Encode + Eq + TypeInfo;
    }

    // Need some storage type that keeps track of sectors, deadlines and terminations.
    // Could be added to this type maybe?
    #[pallet::storage]
    #[pallet::getter(fn storage_providers)]
    pub type StorageProviders<T: Config> =
        StorageMap<_, _, T::AccountId, StorageProviderInfo<T::AccountId, T::PeerId>>;

    #[pallet::event]
    #[pallet::generate_deposit(fn deposit_event)]
    pub enum Event<T: Config> {
        /// This event is emitted when a new storage provider is initialized.
        StorageProviderCreated { owner: T::AccountId },
        /// This event is emitted when a storage provider changes its `PeerId`.
        PeerIdChanged {
            storage_provider: T::AccountId,
            new_peer_id: T::PeerId,
        },
        /// This event is emitted when a storage provider changes its owner.
        OwnerAddressChanged {
            storage_provider: T::AccountId,
            new_owner: T::AccountId,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// Error emitted when the provided information for the storage provider is invalid.
        StorageProviderInfoError,
        /// Error emitted when trying to get info on a storage provider that does not exist.
        StorageProviderNotFound,
        /// Error emitted when doing a privileged call and the signer does not match.
        InvalidSigner,
        /// Error emitted when trying to create a storage provider that is already indexed.
        DuplicateStorageProvider,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Register a new storage provider
        #[pallet::call_index(0)]
        // #[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
        // TODO(aidan46, no-ref, 2024-06-04): Determine applicable weights.
        pub fn create_storage_provider(
            origin: OriginFor<T>,
            peer_id: T::PeerId,
            window_post_proof_type: RegisteredPoStProof,
        ) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer.
            let owner = ensure_signed(origin)?;

            // This means the storage provider is already registered.
            ensure!(
                !StorageProviders::<T>::contains_key(&owner),
                Error::<T>::DuplicateStorageProvider
            );

            // Generate some storage_provider id and insert into `StorageProviders` storage
            let storage_provider_info =
                StorageProviderInfo::new(owner.clone(), peer_id.clone(), window_post_proof_type)
                    .map_err(|_| Error::<T>::StorageProviderInfoError)?;
            StorageProviders::<T>::insert(owner.clone(), storage_provider_info);
            Self::deposit_event(Event::StorageProviderCreated { owner });
            Ok(().into())
        }

        /// Update PeerId for a Storage Provider.
        #[pallet::call_index(1)]
        // #[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
        // TODO(aidan46, no-ref, 2024-06-04): Determine applicable weights.
        pub fn change_peer_id(
            origin: OriginFor<T>,
            peer_id: T::PeerId,
        ) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer.
            let owner = ensure_signed(origin)?;

            // Fails if the SP has not been registered.
            let sp =
                StorageProviders::<T>::get(&owner).ok_or(Error::<T>::StorageProviderNotFound)?;

            // Ensure caller is the owner of SP
            ensure!(owner == sp.owner, Error::<T>::InvalidSigner);

            StorageProviders::<T>::mutate(&owner, |info| {
                // Can safely unwrap this because of previous `get` check
                let sp_info = info.as_mut().unwrap();

                log::debug!("Updating peer id for {owner:?}");

                sp_info.peer_id = peer_id.clone();

                Self::deposit_event(Event::PeerIdChanged {
                    storage_provider: owner.clone(),
                    new_peer_id: peer_id,
                });
                Ok(().into())
            })
        }

        /// Update the owner address for a Storage Provider.
        #[pallet::call_index(2)]
        // #[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
        // TODO(aidan46, no-ref, 2024-06-04): Determine applicable weights.
        pub fn change_owner_address(
            origin: OriginFor<T>,
            new_owner: T::AccountId,
        ) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer.
            let storage_provider = ensure_signed(origin)?;

            // Extract storage provider
            match StorageProviders::<T>::try_get(&storage_provider) {
                Ok(info) => {
                    // Ensure storage_provider is the owner of the storage_provider
                    ensure!(storage_provider == info.owner, Error::<T>::InvalidSigner);

                    // Ensure no storage provider is associated with the new owner
                    ensure!(
                        !StorageProviders::<T>::contains_key(&new_owner),
                        Error::<T>::DuplicateStorageProvider
                    );

                    let new_info = info.change_owner(new_owner.clone());

                    // Insert new storage provider info
                    StorageProviders::<T>::insert(new_owner.clone(), new_info);

                    // Remove old storage provider entry
                    StorageProviders::<T>::remove(storage_provider.clone());

                    // Emit event
                    Self::deposit_event(Event::OwnerAddressChanged {
                        storage_provider: storage_provider.clone(),
                        new_owner,
                    });

                    Ok(().into())
                }
                Err(..) => Err(Error::<T>::StorageProviderNotFound.into()),
            }
        }

        /// Used by the storage provider to submit their Proof-of-Spacetime
        #[pallet::call_index(3)]
        // #[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
        // TODO(aidan46, no-ref, 2024-06-04): Determine applicable weights.
        pub fn submit_windowed_post(
            origin: OriginFor<T>,
            _deadline: u64,
            _partitions: Vec<u64>,
            _proofs: Vec<PoStProof>,
        ) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer.
            let _who = ensure_signed(origin)?;
            // TODO(@aidan46, no-ref, 2024-06-04): Implement submit windowed PoSt functionality
            unimplemented!("Submit windowed PoSt is not implemented yet")
        }

        /// Used to declare a set of sectors as "faulty," indicating that the next PoSt for those sectors'
        /// deadline will not contain a proof for those sectors' existence.
        #[pallet::call_index(4)]
        // #[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
        // TODO(aidan46, no-ref, 2024-06-05): Determine applicable weights.
        pub fn declare_faults(
            origin: OriginFor<T>,
            _deadline: u64,
            _partition: u64,
            _sectors: Vec<u64>,
        ) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer.
            let _who = ensure_signed(origin)?;
            // TODO(@aidan46, no-ref, 2024-06-05): Implement declare faults functionality
            unimplemented!("Declare faults is not implemented yet")
        }

        /// Used by a Storage Provider to declare a set of faulty sectors as "recovering," indicating that the
        /// next PoSt for those sectors' deadline will contain a proof for those sectors' existence.
        #[pallet::call_index(5)]
        // #[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
        // TODO(aidan46, no-ref, 2024-06-05): Determine applicable weights.
        pub fn declare_faults_recovered(
            origin: OriginFor<T>,
            _deadline: u64,
            _partition: u64,
            _sectors: Vec<u64>,
        ) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer.
            let _who = ensure_signed(origin)?;
            // TODO(@aidan46, no-ref, 2024-06-05): Implement declare faults recovered functionality
            unimplemented!("Declare faults recovered is not implemented yet")
        }

        /// Pledges the storage provider to seal and commit some new sectors.
        #[pallet::call_index(6)]
        // #[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
        // TODO(aidan46, no-ref, 2024-06-05): Determine applicable weights.
        pub fn pre_commit_sector(
            origin: OriginFor<T>,
            _sectors: SectorPreCommitInfo,
        ) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer.
            let _who = ensure_signed(origin)?;
            // TODO(@aidan46, no-ref, 2024-06-05): Implement pre commit sector functionality
            unimplemented!("Pre commit sector is not implemented yet")
        }

        /// Checks state of the corresponding sector pre-commitments and verifies aggregate proof of replication
        /// of these sectors. If valid, the sectors' deals are activated.
        #[pallet::call_index(8)]
        // #[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
        // TODO(aidan46, no-ref, 2024-06-05): Determine applicable weights.
        pub fn prove_commit_sector(
            origin: OriginFor<T>,
            _sector_number: SectorNumber,
            _proof: Vec<u8>,
        ) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer.
            let _who = ensure_signed(origin)?;
            // TODO(@aidan46, no-ref, 2024-06-07): Implement prove commit sector functionality
            unimplemented!("Prove commit sector is not implemented yet")
        }
    }
}
