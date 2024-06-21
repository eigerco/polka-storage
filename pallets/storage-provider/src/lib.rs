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
use scale_info::prelude::string::String;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod test;

mod proofs;
mod sector;
mod storage_provider;
mod types;

type Cid = String;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use core::fmt::Debug;

    use codec::{Decode, Encode};
    use frame_support::{
        dispatch::DispatchResultWithPostInfo,
        ensure,
        pallet_prelude::*,
        traits::{Currency, ReservableCurrency},
    };
    use frame_system::{ensure_signed, pallet_prelude::*, Config as SystemConfig};
    use scale_info::TypeInfo;

    use crate::{
        proofs::{
            assign_proving_period_offset, current_deadline_index, current_proving_period_start,
            RegisteredPoStProof,
        },
        storage_provider::{StorageProviderInfo, StorageProviderState},
    };

    /// Allows to extract Balance of an account via the Config::Currency associated type.
    /// BalanceOf is a sophisticated way of getting an u128.
    type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as SystemConfig>::AccountId>>::Balance;

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

        #[pallet::constant] // put the constant in metadata
        /// Proving period for submitting Window PoSt, 24 hours is blocks
        type WPoStProvingPeriod: Get<BlockNumberFor<Self>>;

        #[pallet::constant] // put the constant in metadata
        /// Window PoSt challenge window (default 30 minutes in blocks)
        type WPoStChallengeWindow: Get<BlockNumberFor<Self>>;
    }

    /// Need some storage type that keeps track of sectors, deadlines and terminations.
    #[pallet::storage]
    #[pallet::getter(fn storage_providers)]
    pub type StorageProviders<T: Config> = StorageMap<
        _,
        _,
        T::AccountId,
        StorageProviderState<T::PeerId, BalanceOf<T>, BlockNumberFor<T>>,
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
        /// Emitted when a type conversion fails.
        ConversionError,
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

            let proving_period = T::WPoStProvingPeriod::get();

            
            let current_block = <frame_system::Pallet<T>>::block_number();

            let offset = assign_proving_period_offset::<T::AccountId, BlockNumberFor<T>>(
                &owner,
                current_block,
                proving_period,
            )
            .map_err(|_| Error::<T>::ConversionError)?;

            let period_start = current_proving_period_start(current_block, offset, proving_period);

            let deadline_idx =
                current_deadline_index(current_block, period_start, T::WPoStChallengeWindow::get());

            let info = StorageProviderInfo::new(peer_id, window_post_proof_type);

            let state = StorageProviderState::new(&info, period_start, deadline_idx);

            StorageProviders::<T>::insert(&owner, state);

            // Emit event
            Self::deposit_event(Event::StorageProviderRegistered { owner, info });

            Ok(().into())
        }
    }
}
