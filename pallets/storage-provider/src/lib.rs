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

pub use pallet::{Config, Pallet};

#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod test;

mod types;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use crate::types::{RegisteredPoStProof, StorageProviderInfo, StorageProviderState};

    use codec::{Decode, Encode};
    use core::fmt::Debug;
    use frame_support::{
        dispatch::DispatchResultWithPostInfo,
        ensure,
        pallet_prelude::{IsType, StorageMap},
        traits::{Currency, ReservableCurrency},
    };
    use frame_system::{ensure_signed, pallet_prelude::OriginFor, Config as SystemConfig};
    use scale_info::TypeInfo;

    // Allows to extract Balance of an account via the Config::Currency associated type.
    // BalanceOf is a sophisticated way of getting an u128.
    type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as SystemConfig>::AccountId>>::Balance;

    // TODO(@aidan46, no-ref, 2023-06-17): Set a logical value for proving period start, in Filecoin this is a random number in a range.
    const _PROVING_PERIOD_OFFSET: u32 = 5;
    /// TODO(@aidan46, no-ref, 2023-06-17): Set a logical challenge window. Filecoin uses half an hour (=48 per day).
    const _WPOST_CHALLENGE_WINDOW: u32 = 48;

    #[pallet::pallet]
    #[pallet::without_storage_info] // Allows to define storage items without fixed size
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Peer ID is derived by hashing an encoded public key.
        /// Usually represented in bytes.
        /// https://github.com/libp2p/specs/blob/2ea41e8c769f1bead8e637a9d4ebf8c791976e8a/peer-ids/peer-ids.md#peer-ids
        type PeerId: Clone + Debug + Decode + Encode + Eq + TypeInfo;

        /// Currency mechanism, used for collateral
        type Currency: ReservableCurrency<Self::AccountId>;
    }

    // Need some storage type that keeps track of sectors, deadlines and terminations.
    // Could be added to this type maybe?
    #[pallet::storage]
    #[pallet::getter(fn storage_providers)]
    pub type StorageProviders<T: Config> = StorageMap<
        _,
        _,
        T::AccountId,
        StorageProviderState<T::PeerId, <T as SystemConfig>::Block, BalanceOf<T>>,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(fn deposit_event)]
    pub enum Event<T: Config> {
        /// Emitted when a new storage provider is registered.
        StorageProviderRegistered {
            owner: T::AccountId,
            info: StorageProviderInfo<T::PeerId>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// Emitted when a storage provider is trying to be registered
        /// but there is already storage provider registered for that `AccountId`.
        StorageProviderExists,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        pub fn register_storage_provider(
            origin: OriginFor<T>,
            peer_id: T::PeerId,
            window_post_proof_type: RegisteredPoStProof,
        ) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer
            // This will be the owner of the storage provider
            let owner = ensure_signed(origin)?;

            // Ensure that the storage provider does not exist yet
            ensure!(
                !StorageProviders::<T>::contains_key(&owner),
                Error::<T>::StorageProviderExists
            );

            let info = StorageProviderInfo::new(peer_id, window_post_proof_type);
            let state = StorageProviderState::new(&info);
            StorageProviders::<T>::insert(&owner, state);
            Self::deposit_event(Event::StorageProviderRegistered { owner, info });
            Ok(().into())
        }
    }
}
