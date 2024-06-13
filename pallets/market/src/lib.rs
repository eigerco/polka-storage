//! # Market Pallet
//!
//! # Overview
//!
//! Market Pallet provides functions for:
//! - storing balances of Storage Clients and Storage Providers to handle deal collaterals and payouts
//! 

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use codec::{Decode, Encode};
    use frame_support::{
        dispatch::DispatchResult,
        pallet_prelude::*,
        sp_runtime::{RuntimeDebug,traits::{AccountIdConversion}},
        traits::{Currency, ReservableCurrency,ExistenceRequirement::KeepAlive,ExistenceRequirement::AllowDeath},
        PalletId,
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

        /// PalletId used to derive AccountId which stores funds of the Market Participants
        #[pallet::constant]
        type PalletId: Get<PalletId>;
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
        StorageMap<_, _, T::AccountId, BalanceEntry<BalanceOf<T>>, ValueQuery>;

    #[pallet::event]
    // #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        // TODO(@th7nder,13/06/2024): 
        // - add output balance
        // - add failure event
        // - add withdrawal event 
        // - verify what happens (nothing the fees are paid by the executor...)
        BalanceAdded(T::AccountId),
    }

    #[pallet::error]
    pub enum Error<T> {
        InsufficientFunds,
    }

    /// Extrinsics exposed by the pallet
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        pub fn add_balance(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            BalanceTable::<T>::try_mutate(
                &caller,
                |balance| -> DispatchResult {
                    T::Currency::transfer(&caller, &Self::account_id(), amount, KeepAlive)?;
                    balance.deposit += amount;
                    Ok(())
                }
            )?;

            Ok(())
        }

        pub fn withdraw_balance(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            BalanceTable::<T>::try_mutate(
                &caller,
                |balance| -> DispatchResult {
                    // TODO(@th7nder,13/06/2024): add checking if the funds are not locked as collateral
                    balance.deposit -= amount;

                    // NOTE(@th7nder,13/06/2024): Edge Case
                    // We allow death to be able to withdraw the funds if only 1 SP has allocated them
                    // The Market Pallet account will be reaped.
                    T::Currency::transfer(&Self::account_id(), &caller, amount, AllowDeath)?;
                    Ok(())
                }
            )?;

            Ok(())
        }
    }

    /// Functions exposed by the pallet
    impl<T: Config> Pallet<T> {
        /// Account Id of the Market
        ///
        /// This actually does computation. 
        /// If you need to keep using it, make sure you cache it and call it once.
        pub fn account_id() -> T::AccountId {
            T::PalletId::get().into_account_truncating()
        }
    }
}
