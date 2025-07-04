use frame_support::traits::Hooks;
use frame_system::pallet_prelude::BlockNumberFor;

pub struct Pallet<T: Config>(crate::Pallet<T>);
pub trait Config:
    crate::Config + pallet_balances::Config + pallet_proofs::Config + frame_system::Config
{
}

impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
    fn on_initialize(n: BlockNumberFor<T>) -> frame_support::weights::Weight {
        crate::Pallet::<T>::on_initialize(n)
    }

    fn on_finalize(n: BlockNumberFor<T>) {
        crate::Pallet::<T>::on_finalize(n)
    }
}
