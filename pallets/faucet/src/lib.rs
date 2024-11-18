#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod test;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use frame_support::{
        pallet_prelude::*,
        traits::{Currency, ReservableCurrency},
    };
    use frame_system::{ensure_none, pallet_prelude::*};

    /// Allows to extract Balance of an account via the Config::Currency associated type.
    /// BalanceOf is a sophisticated way of getting an u128.
    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The currency mechanism.
        type Currency: ReservableCurrency<Self::AccountId>;

        /// The amount that is dispensed in planck's
        #[pallet::constant]
        type FaucetAmount: Get<BalanceOf<Self>>;

        /// How often an account can use the drip function (1 day on testnet)
        #[pallet::constant]
        type FaucetDelay: Get<BlockNumberFor<Self>>;
    }

    /// By default pallet do no allow for unsigned transactions.
    /// Implementing this trait for the faucet Pallet allows unsigned extrinsics to be called.
    /// There is no complicated implementation needed (like checking the call type)
    /// because there is only one transaction in this pallet
    #[pallet::validate_unsigned]
    impl<T: Config> ValidateUnsigned for Pallet<T> {
        type Call = Call<T>;

        fn validate_unsigned(
            _source: TransactionSource,
            _call: &Self::Call,
        ) -> TransactionValidity {
            let current_block = <frame_system::Pallet<T>>::block_number();
            ValidTransaction::with_tag_prefix("pallet-faucet")
                .and_provides(current_block)
                .build()
        }
    }

    /// Keeps track of when accounts last used the drip function.
    #[pallet::storage]
    #[pallet::getter(fn drips)]
    pub type Drips<T: Config> = StorageMap<_, _, T::AccountId, BlockNumberFor<T>>;

    #[pallet::event]
    #[pallet::generate_deposit(fn deposit_event)]
    pub enum Event<T: Config> {
        /// Emitted when an account uses the drip function successfully.
        Dripped {
            who: T::AccountId,
            when: BlockNumberFor<T>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// Emitted when an account tries to call the drip function more than 1x in 24 hours.
        FaucetUsedRecently,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        pub fn drip(origin: OriginFor<T>, account: T::AccountId) -> DispatchResult {
            let _ = ensure_none(origin)?;
            let current_block = <frame_system::Pallet<T>>::block_number();
            if let Some(faucet_block) = Self::drips(&account) {
                ensure!(current_block >= (faucet_block + T::FaucetDelay::get()), {
                    log::error!("{account:?} has recently used the faucet");
                    Error::<T>::FaucetUsedRecently
                });
            }
            log::info!("Dripping {:?} to {account:?}", T::FaucetAmount::get());
            let imbalance = T::Currency::deposit_creating(&account, T::FaucetAmount::get());
            drop(imbalance);
            Drips::<T>::insert(account.clone(), current_block);
            Self::deposit_event(Event::<T>::Dripped {
                who: account,
                when: current_block,
            });
            Ok(())
        }
    }
}
