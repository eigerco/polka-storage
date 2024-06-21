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

mod proofs;
mod sector;
mod storage_provider;
mod types;

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
        sector::{SectorPreCommitInfo, SectorPreCommitOnChainInfo, SECTORS_MAX},
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

        type DealID: Copy + Debug + Decode + Encode + PartialEq + TypeInfo;

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
        StorageProviderState<T::PeerId, BalanceOf<T>, BlockNumberFor<T>, T::DealID>,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(fn deposit_event)]
    pub enum Event<T: Config> {
        /// Emitted when a new storage provider is registered.
        StorageProviderRegistered {
            owner: T::AccountId,
            info: StorageProviderInfo<T::PeerId>,
        },
        /// Emitted when a storage provider pre commits some sectors.
        SectorPreCommitted {
            owner: T::AccountId,
            sector: SectorPreCommitInfo<BlockNumberFor<T>, T::DealID>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// Emitted when a storage provider is trying to be registered
        /// but there is already storage provider registered for that `AccountId`.
        StorageProviderExists,
        /// Emitted when a type conversion fails.
        ConversionError,
        /// Emitted when an account tries to call a storage provider
        /// extrinsic but is not registered as one.
        StorageProviderNotFound,
        /// Emitted when trying to access an invalid sector.
        InvalidSector,
        /// Emitted when submitting an invalid proof type.
        InvalidProofType,
        /// Emitted when there is not enough funds to run an extrinsic.
        NotEnoughFunds,
        /// Emitted when a storage provider tries to commit more sectors than MAX_SECTORS.
        MaxPreCommittedSectorExceeded,
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

        /// Pledges the storage provider to seal and commit some new sector
        /// TODO(@aidan46, no-ref, 2024-06-20): Add functionality to allow for batch pre commit
        pub fn pre_commit_sector(
            origin: OriginFor<T>,
            sector: SectorPreCommitInfo<BlockNumberFor<T>, T::DealID>,
        ) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer
            // This will be the owner of the storage provider
            let owner = ensure_signed(origin)?;

            let sp = StorageProviders::<T>::try_get(&owner)
                .map_err(|_| Error::<T>::StorageProviderNotFound)?;

            if sector.sector_number > SECTORS_MAX {
                return Err(Error::<T>::InvalidSector.into());
            }

            ensure!(
                sp.info.window_post_proof_type == sector.seal_proof.registered_window_post_proof(),
                Error::<T>::InvalidProofType
            );

            let balance = T::Currency::total_balance(&owner);
            let deposit = calculate_pre_commit_deposit::<T>();

            ensure!(balance >= deposit, Error::<T>::NotEnoughFunds);

            T::Currency::reserve(&owner, deposit)?;

            StorageProviders::<T>::try_mutate(&owner, |maybe_sp| -> DispatchResultWithPostInfo {
                let sp = maybe_sp
                    .as_mut()
                    .ok_or(Error::<T>::StorageProviderNotFound)?;
                sp.add_pre_commit_deposit(deposit);
                sp.put_precommitted_sector(SectorPreCommitOnChainInfo::new(
                    sector.clone(),
                    deposit,
                    <frame_system::Pallet<T>>::block_number(),
                ))
                .map_err(|_| Error::<T>::MaxPreCommittedSectorExceeded)?;
                Ok(().into())
            })?;

            Self::deposit_event(Event::SectorPreCommitted { owner, sector });
            Ok(().into())
        }
    }

    /// Calculate the required pre commit deposit amount
    fn calculate_pre_commit_deposit<T: Config>() -> BalanceOf<T> {
        1u32.into() // TODO(@aidan46, no-ref, 2024-06-24): Set a logical value or calculation
    }
}
