//! # Market Pallet
//!
//! # Overview
//!
//! Market Pallet provides functions for:
//! - storing balances of Storage Clients and Storage Providers to handle deal collaterals and payouts

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod test;

// TODO(@th7nder,#77,14/06/2024): take the pallet out of dev mode
#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use codec::{Decode, Encode};
    use frame_support::{
        dispatch::DispatchResult,
        ensure,
        pallet_prelude::*,
        sp_runtime::{
            traits::{AccountIdConversion, CheckedAdd, CheckedSub},
            ArithmeticError, RuntimeDebug,
        },
        traits::{
            Currency,
            ExistenceRequirement::{AllowDeath, KeepAlive},
            ReservableCurrency,
        },
        PalletId,
    };
    use frame_system::{pallet_prelude::*, Config as SystemConfig};
    use scale_info::TypeInfo;

    /// Allows to extract Balance of an account via the Config::Currency associated type.
    /// BalanceOf is a sophisticated way of getting an u128.
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
    /// We do not use the ReservableCurrency::reserve mechanism,
    /// as the Market works as a liaison between Storage Providers and Storage Clients.
    /// Market has its own account on which funds of all parties are stored.
    /// It's Market reposibility to manage deposited funds, lock/unlock and pay them out when necessary.
    #[derive(
        Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, Default, TypeInfo, MaxEncodedLen,
    )]
    pub struct BalanceEntry<Balance> {
        /// Amount of Balance that has been deposited for future deals/earned from deals.
        /// It can be withdrawn at any time.
        pub(crate) free: Balance,
        /// Amount of Balance that has been staked as Deal Collateral
        /// It's locked to a deal and cannot be withdrawn until the deal ends.
        pub(crate) locked: Balance,
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn storage_provider_claims)]
    pub type BalanceTable<T: Config> =
        StorageMap<_, _, T::AccountId, BalanceEntry<BalanceOf<T>>, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Market Participant deposited free balance to the Market Account
        BalanceAdded {
            who: T::AccountId,
            amount: BalanceOf<T>,
        },
        /// Market Participant withdrawn their free balance from the Market Account
        BalanceWithdrawn {
            who: T::AccountId,
            amount: BalanceOf<T>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// When a Market Participant tries to withdraw more
        /// funds than they have available on the Market, because:
        /// - they never deposited the amount they want to withdraw
        /// - the funds they deposited were locked as part of a deal
        InsufficientFreeFunds,
    }

    /// Extrinsics exposed by the pallet
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Transfers `amount` of Balance from the `origin` to the Market Pallet account.
        /// It is marked as _free_ in the Market bookkeeping.
        /// Free balance can be withdrawn at any moment from the Market.
        pub fn add_balance(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            BalanceTable::<T>::try_mutate(&caller, |balance| -> DispatchResult {
                balance.free = balance
                    .free
                    .checked_add(&amount)
                    .ok_or(ArithmeticError::Overflow)?;
                T::Currency::transfer(&caller, &Self::account_id(), amount, KeepAlive)?;

                Self::deposit_event(Event::<T>::BalanceAdded {
                    who: caller.clone(),
                    amount,
                });
                Ok(())
            })?;

            Ok(())
        }

        /// Transfers `amount` of Balance from the Market Pallet account to the `origin`.
        /// Only _free_ balance can be withdrawn.
        pub fn withdraw_balance(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            BalanceTable::<T>::try_mutate(&caller, |balance| -> DispatchResult {
                ensure!(balance.free >= amount, Error::<T>::InsufficientFreeFunds);
                balance.free = balance
                    .free
                    .checked_sub(&amount)
                    .ok_or(ArithmeticError::Underflow)?;
                // The Market Pallet account will be reaped if no one is participating in the market.
                T::Currency::transfer(&Self::account_id(), &caller, amount, AllowDeath)?;

                Self::deposit_event(Event::<T>::BalanceWithdrawn {
                    who: caller.clone(),
                    amount,
                });
                Ok(())
            })?;

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
