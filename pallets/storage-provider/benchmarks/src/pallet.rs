use frame_support::traits::OnInitialize;
use frame_system::pallet_prelude::BlockNumberFor;

pub struct Pallet<T: Config>(pallet_storage_provider::Pallet<T>);
pub trait Config:
    pallet_storage_provider::Config
    + pallet_balances::Config
    + pallet_market::Config
    + frame_system::Config
{
}

impl<T: Config> OnInitialize<BlockNumberFor<T>> for Pallet<T> {
    fn on_initialize(n: BlockNumberFor<T>) -> frame_support::weights::Weight {
        pallet_storage_provider::Pallet::<T>::on_initialize(n)
    }
}
