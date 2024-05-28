//! # Collator Power Pallet
//!
//! Collator Power Pallet manages collator's `Power` which is used
//! in the [selection process](https://github.com/eigerco/polka-disk/blob/main/doc/research/parachain/parachain-implementation.md#collator-selection-pallet)
//! of a [Collator Node](https://github.com/eigerco/polka-disk/blob/main/doc/research/parachain/parachain-implementation.md#collator-node).
//!
//! # Overview
//!
//! The Collator Power Pallet provides functions for:
//! - ...

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use codec::{Decode, Encode};
    use frame_support::{
        dispatch::DispatchResultWithPostInfo,
        pallet_prelude::*,
        sp_runtime::RuntimeDebug,
        traits::{Currency, ReservableCurrency},
    };
    use frame_system::{pallet_prelude::*, Config as SystemConfig};
    use scale_info::TypeInfo;

    // Allows to extract Balance of an account via the Config::Currency associated type.
    // BalanceOf is a sophisticated way of getting an u128.
    type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as SystemConfig>::AccountId>>::Balance;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The currency mechanism.
        /// Used for staking Collaterals by Storage Providers
        type Currency: ReservableCurrency<Self::AccountId>;

        /// Unit of Storage Power of a Storage Provider
        /// E.g. `u128`, used as `number of bytes` for a given SP.
        type StoragePower: Parameter + Member + Clone + MaxEncodedLen;

        /// A stable ID for a Collator
        type CollatorId: Parameter + Member + Ord + MaxEncodedLen;

        /// A stable ID for a Storage Provider
        type StorageProviderId: Parameter + Member + Ord + MaxEncodedLen;
    }

    #[derive(
        Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, Default, TypeInfo, MaxEncodedLen,
    )]
    pub struct StorageProviderClaim<CollatorId: Ord, Balance, StoragePower> {
        /// Number of bytes stored by a Storage Provider
        raw_bytes_power: StoragePower,
        /// Stores how much currency was staked on a particular collator
        delegates: BoundedBTreeMap<CollatorId, Balance, ConstU32<10>>
    }

    #[derive(
        Clone, Eq, PartialEq, Encode, Decode, Default, RuntimeDebug, TypeInfo, MaxEncodedLen,
    )]
    pub struct CollatorClaim<Balance, StorageProviderId: Ord> {
        /// Amount of Currency that was pledged by a Collator to participate in block producer selection
        pledged_own_collateral: Balance,
        /// Stores how much every Storage Provider has staked on a given Collator
        delegated_collateral: BoundedBTreeMap<StorageProviderId, Balance, ConstU32<100>>
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn collator_claims)]
    pub type CollatorClaims<T: Config> =
        StorageMap<_, _, T::CollatorId, CollatorClaim<BalanceOf<T>, T::StorageProviderId>>;

    #[pallet::storage]
    #[pallet::getter(fn storage_provider_claims)]
    pub type StorageProviderClaims<T: Config> =
        StorageMap<_, _, T::StorageProviderId, StorageProviderClaim<T::CollatorId, BalanceOf<T>, T::StoragePower>>;

    #[pallet::event]
    // #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Indicates that a new storage provider has been registered.
        /// Newly created storage provider does not have any Power.
        /// Power is updated after the Storage Provider proves it has storage available.
        StorageProviderRegistered(T::AccountId),
    }

    #[pallet::error]
    pub enum Error<T> {
        /// If there is an entry in claims map, connected to the AccountId that tries to be registered as a Storage Provider.
        StorageProviderAlreadyRegistered,
    }

    /// Extrinsics exposed by the pallet
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Registers account as a Storage Provider
        /// Initially, Storage Provider has 0 Storage Power.
        /// It needs to be added via UpdateStoragePower.
        pub fn register_storage_provider(_origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            todo!()
        }

        /// After Storage Provider proved a sector, calls this method to update the bookkeeping about available power.
        pub fn update_storage_power(
            _storage_provider: OriginFor<T>,
            _raw_delta_bytes: T::StoragePower,
        ) -> DispatchResultWithPostInfo {
            todo!()
        }

        /// Adds collator to the active collator set, the set is taken into account when `pallet collator selection` makes decision.
        /// Collator initially has zero power.
        /// Called with `root` privileges for now.
        pub fn register_collator(_origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            todo!()
        }

        /// Sets a lock on `amount` of currency from a given `storageProvider`
        /// Effectively staking `amount` on a `collator`.
        /// Saves how much was staked, and later can be returned by `getActiveCollators()`
        pub fn nominate_collator(
            _storage_provider: OriginFor<T>,
            _collator: T::CollatorId,
            _amount: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            todo!()
        }

        /// Removes the `collator`'s nomination backed by `storageProvider`.
        /// Returns pledged funds back to the `storageProvider`.
        /// Remove a lock on `amount` of currency from a given `collator`.
        /// Returns funds to the `storageProvider`.
        pub fn denominate_collator(
            _storage_provider: OriginFor<T>,
            _collator: T::CollatorId,
        ) -> DispatchResultWithPostInfo {
            todo!()
        }

        /// Pledges a certain amount of balance for a more likelihood to be selected to be a block producer.
        /// It's called by collator on its' own, as it's not always pledging it's entire balance.
        pub fn pledge_collateral(_collator: OriginFor<T>, _amount: BalanceOf<T>) -> DispatchResultWithPostInfo {
            todo!()
        }
    }

    /// Functions exposed by the pallet
    /// e.g. `pallet-collator-selection` used them to make decision about the next block producer
    impl<T: Config> Pallet<T> {
        /// Returns map of collator_id -> pledged collateral + its balance.
        /// Essentially:
        /// - adds pledged collateral from CollatorClaims
        /// - goes through all of the Storage Provider 
        pub fn active_collators() {
            todo!();
        }

        /// Gets total power acquired by a collator
        /// Total Power = ballance pledged by a collator and collateral staked on it by Storage Providers.
        pub fn get_power(_collator: T::CollatorId) {
            todo!();
        }
    }
}
