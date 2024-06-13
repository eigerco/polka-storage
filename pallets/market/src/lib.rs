//! # Market Pallet
//!
//! # Overview
//!
//! Market Pallet provides functions for:
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
        type Currency: ReservableCurrency<Self::AccountId>;
    }

    /// Stores balances info for both Storage Providers and Storage Users
    #[derive(
        Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, Default, TypeInfo, MaxEncodedLen,
    )]
    pub struct BalanceEntry<Balance> {
        /// Amount of Balance that has been deposited for future deals
        deposit: Balance,
        /// Amount of Balance that has been staked as Deal Collateral
        locked: Balance,
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn storage_provider_claims)]
    pub type BalanceTable<T: Config> =
        StorageMap<_, _, T::AccountId, BalanceEntry<BalanceOf<T>>>;

    #[pallet::event]
    // #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        BalanceAdded(T::AccountId),
    }

    #[pallet::error]
    pub enum Error<T> {
        InsufficientFunds,
    }

    /// Extrinsics exposed by the pallet
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        pub fn add_balance(_origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            todo!()
        }
    }

    /// Functions exposed by the pallet
    impl<T: Config> Pallet<T> {
      
    }
}
