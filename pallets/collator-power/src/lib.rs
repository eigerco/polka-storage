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
        type CollatorId: Parameter + Member + MaxEncodedLen;

        /// A stable ID for a Storage Provider
        type StorageProviderId: Parameter + Member + MaxEncodedLen;
    }

    #[derive(
        Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, Default, TypeInfo, MaxEncodedLen,
    )]
    pub struct StorageProviderClaim<StoragePower> {
        /// Number of bytes stored by a miner
        raw_bytes_power: StoragePower,
    }

    #[derive(
        Clone, Eq, PartialEq, Encode, Decode, Default, RuntimeDebug, TypeInfo, MaxEncodedLen,
    )]
    pub struct CollatorClaim<Balance> {
        /// Amount of Currency that was pledged by a Collator to participate in block producer selection
        pledged_collateral: Balance,
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn collator_claims)]
    pub type CollatorClaims<T: Config> =
        StorageMap<_, _, T::CollatorId, CollatorClaim<BalanceOf<T>>>;

    #[pallet::storage]
    #[pallet::getter(fn storage_provider_claims)]
    pub type StorageProviderClaims<T: Config> =
        StorageMap<_, _, T::StorageProviderId, StorageProviderClaim<T::StoragePower>>;

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

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        pub fn register_storage_provider(_origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            todo!()
        }
    }
}
