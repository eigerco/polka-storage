#![cfg(feature = "runtime-benchmarks")]

use frame_benchmarking::v2::*;
use frame_system::RawOrigin;

use super::*;
#[allow(unused)]
use crate::Pallet as RandomnessPallet;

#[frame_benchmarking::v2::benchmarks(
    where T: crate::Config
)]
mod benchmarks {
    use super::*;

    /// The worst case scenario is when the history is full.
    /// Instead of just adding the randomness value, it requires removing the oldest value too.
    #[benchmark]
    fn set_author_vrf() {
        use frame_system::pallet_prelude::BlockNumberFor;

        use crate::pallet::AuthorVrfHistory;

        // Add 256 blocks so we trigger the remove into insert logic
        for block_number in 0..=256u32 {
            AuthorVrfHistory::<T>::insert::<BlockNumberFor<T>, T::Hash>(
                block_number.into(),
                Default::default(),
            );
        }

        #[extrinsic_call]
        _(RawOrigin::None);

        let author_vrf = T::AuthorVrfGetter::get_author_vrf();
        assert!(author_vrf.is_some());
    }

    impl_benchmark_test_suite! {
        RandomnessPallet,
        crate::mock::new_test_ext(),
        crate::mock::Test,
    }
}
