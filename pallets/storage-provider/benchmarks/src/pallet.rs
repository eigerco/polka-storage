use frame_support::traits::Hooks;
use frame_system::pallet_prelude::BlockNumberFor;

pub struct Pallet<T: Config>(pallet_storage_provider::Pallet<T>);
pub trait Config:
    pallet_storage_provider::Config
    + pallet_balances::Config
    + pallet_proofs::Config
    + frame_system::Config
{
    // type Currency: ReservableCurrency<Self::AccountId>;
}

impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
    fn on_initialize(n: BlockNumberFor<T>) -> frame_support::weights::Weight {
        pallet_storage_provider::Pallet::<T>::on_initialize(n)
    }

    fn on_finalize(n: BlockNumberFor<T>) {
        pallet_storage_provider::Pallet::<T>::on_finalize(n)
    }
}
