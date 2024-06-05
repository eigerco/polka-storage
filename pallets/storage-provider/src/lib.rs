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
mod benchmarking;

pub use pallet::{Config, Pallet};

use codec::{Decode, Encode};
use scale_info::prelude::string::String;
use scale_info::TypeInfo;

#[derive(Decode, Encode, TypeInfo)]
pub struct StorageProviderInfo<
    AccountId: Encode + Decode + Eq + PartialEq,
    PeerId: Encode + Decode + Eq + PartialEq,
    StoragePower: Encode + Decode + Eq + PartialEq,
> {
    /// The owner of this storage_provider.
    owner: AccountId,
    /// storage_provider's libp2p peer id in bytes.
    peer_id: PeerId,
    /// The total power the storage provider has
    total_raw_power: StoragePower,
    /// The price of storage (in DOT) for each block the storage provider takes for storage.
    // TODO(aidan46, no-ref, 2024-06-04): Use appropriate type
    price_per_block: String,
}

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use super::StorageProviderInfo;

    use codec::{Decode, Encode};
    use core::fmt::Debug;
    use frame_support::dispatch::DispatchResultWithPostInfo;
    use frame_support::ensure;
    use frame_support::pallet_prelude::{IsType, StorageMap};
    use frame_support::traits::{Currency, ReservableCurrency};
    use frame_system::pallet_prelude::OriginFor;
    use frame_system::{ensure_signed, Config as SystemConfig};
    use scale_info::prelude::string::String;
    use scale_info::TypeInfo;

    // Allows to extract Balance of an account via the Config::Currency associated type.
    // BalanceOf is a sophisticated way of getting an u128.
    type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as SystemConfig>::AccountId>>::Balance;

    #[pallet::pallet]
    #[pallet::without_storage_info] // Allows to define storage items without fixed size
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The currency mechanism.
        /// Used for rewards, using `ReservableCurrency` over `Currency` because the rewards will be locked
        /// in this pallet until the storage provider requests the funds through `withdraw_balance`
        type Currency: ReservableCurrency<Self::AccountId>;

        /// Peer ID is derived by hashing an encoded public key.
        /// Usually represented in bytes.
        /// https://github.com/libp2p/specs/blob/master/peer-ids/peer-ids.md#peer-ids
        type PeerId: Clone + Debug + Decode + Encode + Eq + TypeInfo;

        /// Unit of Storage Power of a Miner
        /// E.g. `u128`, used as `number of bytes` for a given SP.
        type StoragePower: Clone + Debug + Decode + Encode + Eq + TypeInfo;
    }

    // Need some storage type that keeps track of sectors, deadlines and terminations.
    // Could be added to this type maybe?
    #[pallet::storage]
    #[pallet::getter(fn storage_providers)]
    pub type StorageProviders<T: Config> = StorageMap<
        _,
        _,
        T::AccountId,
        StorageProviderInfo<T::AccountId, T::PeerId, T::StoragePower>,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
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
        /// Error emitted when trying to get info on a storage provider that does not exist.
        StorageProviderNotFound,
        /// Error emitted when doing a privileged call and the signer does not match.
        InvalidSigner,
        /// Error emitted when trying to create a storage provider that is already indexed.
        DuplicateStorageProvider,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Index a new storage provider
        #[pallet::call_index(0)]
        // #[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
        // TODO(aidan46, no-ref, 2024-06-04): Determine applicable weights.
        pub fn create_storage_provider(
            origin: OriginFor<T>,
            peer_id: T::PeerId,
            total_raw_power: T::StoragePower,
            price_per_block: String,
        ) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer.
            let owner = ensure_signed(origin)?;

            // Generate some storage_provider id and insert into `StorageProviders` storage
            let storage_provider_info = StorageProviderInfo {
                owner: owner.clone(),
                peer_id: peer_id.clone(),
                total_raw_power,
                price_per_block,
            };
            // Probably need some check to make sure the storage provider is legit
            // This means the storage provider exist
            ensure!(
                !StorageProviders::<T>::contains_key(&owner),
                Error::<T>::DuplicateStorageProvider
            );
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
            let storage_provider = ensure_signed(origin)?;

            StorageProviders::<T>::try_mutate(
                &storage_provider,
                |info| -> DispatchResultWithPostInfo {
                    let storage_provider_info =
                        match info.as_mut().ok_or(Error::<T>::StorageProviderNotFound) {
                            Ok(info) => info,
                            Err(e) => {
                                log::error!(
                                    "Could not get info for storage_provider: {storage_provider:?}"
                                );
                                return Err(e.into());
                            }
                        };

                    // Ensure storage_provider is the owner of the storage_provider
                    ensure!(
                        storage_provider == storage_provider_info.owner,
                        Error::<T>::InvalidSigner
                    );

                    log::debug!("Updating peer id for {storage_provider:?}");

                    // Update PeerId
                    storage_provider_info.peer_id = peer_id.clone();

                    Self::deposit_event(Event::PeerIdChanged {
                        storage_provider: storage_provider.clone(),
                        new_peer_id: peer_id,
                    });
                    Ok(().into())
                },
            )
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

                    let new_info = StorageProviderInfo {
                        owner: new_owner.clone(),
                        peer_id: info.peer_id,
                        total_raw_power: info.total_raw_power,
                        price_per_block: info.price_per_block,
                    };

                    // Ensure no storage provider is associated with the new owner
                    ensure!(
                        !StorageProviders::<T>::contains_key(&new_owner),
                        Error::<T>::DuplicateStorageProvider
                    );

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

        // Used by the reward pallet to award a block reward to a storage_provider.
        #[pallet::call_index(3)]
        // #[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
        // TODO(aidan46, no-ref, 2024-06-04): Determine applicable weights.
        pub fn apply_rewards(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer.
            let _who = ensure_signed(origin)?;
            // TODO(@aidan46, no-ref, 2024-06-04): Implement apply rewards functionality
            unimplemented!("Apply rewards is not implemented yet")
        }

        /// This method is used to report a consensus fault by a storage provider.
        #[pallet::call_index(4)]
        // #[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
        // TODO(aidan46, no-ref, 2024-06-04): Determine applicable weights.
        pub fn report_consensus_fault(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer.
            let _who = ensure_signed(origin)?;
            // TODO(@aidan46, no-ref, 2024-06-04): Implement report consensus fault functionality
            unimplemented!("Report consensus fault is not implemented yet")
        }

        /// Used by the storage provider to withdraw available funds earned from block rewards.
        /// If the amount to withdraw is larger than what is available the extrinsic will fail.
        #[pallet::call_index(5)]
        // #[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
        // TODO(aidan46, no-ref, 2024-06-04): Determine applicable weights.
        pub fn withdraw_balance(
            origin: OriginFor<T>,
            _amount: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer.
            let _who = ensure_signed(origin)?;
            // TODO(@aidan46, no-ref, 2024-06-04): Implement withdraw balance functionality
            unimplemented!("Withdraw balance is not implemented yet")
        }

        /// Used by the storage provider to submit their Proof-of-Spacetime
        #[pallet::call_index(6)]
        // #[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
        // TODO(aidan46, no-ref, 2024-06-04): Determine applicable weights.
        pub fn submit_windowed_post(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer.
            let _who = ensure_signed(origin)?;
            // TODO(@aidan46, no-ref, 2024-06-04): Implement submit windowed PoSt functionality
            unimplemented!("Submit windowed PoSt is not implemented yet")
        }

        /// Used to declare a set of sectors as "faulty," indicating that the next PoSt for those sectors'
        /// deadline will not contain a proof for those sectors' existence.
        #[pallet::call_index(7)]
        // #[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
        // TODO(aidan46, no-ref, 2024-06-05): Determine applicable weights.
        pub fn declare_faults(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer.
            let _who = ensure_signed(origin)?;
            // TODO(@aidan46, no-ref, 2024-06-05): Implement declare faults functionality
            unimplemented!("Declare faults is not implemented yet")
        }

        /// Used by a Storage Provider to declare a set of faulty sectors as "recovering," indicating that the 
        /// next PoSt for those sectors' deadline will contain a proof for those sectors' existence.
        #[pallet::call_index(8)]
        // #[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
        // TODO(aidan46, no-ref, 2024-06-05): Determine applicable weights.
        pub fn declare_faults_recovered(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer.
            let _who = ensure_signed(origin)?;
            // TODO(@aidan46, no-ref, 2024-06-05): Implement declare faults recovered functionality
            unimplemented!("Declare faults recovered is not implemented yet")
        }
    }
}
