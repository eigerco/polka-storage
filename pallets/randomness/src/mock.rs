use frame_support::{
    derive_impl,
    traits::{OnFinalize, OnInitialize},
};
use frame_system::{mocking::MockBlock, RawOrigin};
use sp_runtime::{traits::Hash, BuildStorage};

use crate as pallet_randomness;
use crate::GetAuthorVrf;

// Configure a mock runtime to test the pallet.
#[frame_support::runtime]
mod test_runtime {
    #[runtime::runtime]
    #[runtime::derive(
        RuntimeCall,
        RuntimeEvent,
        RuntimeError,
        RuntimeOrigin,
        RuntimeFreezeReason,
        RuntimeHoldReason,
        RuntimeSlashReason,
        RuntimeLockId,
        RuntimeTask
    )]
    pub struct Test;

    #[runtime::pallet_index(0)]
    pub type System = frame_system;
    #[runtime::pallet_index(37)]
    pub type RandomnessModule = pallet_randomness;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = MockBlock<Test>;
    type Nonce = u64;
}

impl crate::Config for Test {
    type AuthorVrfGetter = DummyVrf<Self>;
    type WeightInfo = ();
}

pub struct DummyVrf<C>(core::marker::PhantomData<C>)
where
    C: frame_system::Config;

impl<C> GetAuthorVrf<C::Hash> for DummyVrf<C>
where
    C: frame_system::Config,
{
    fn get_author_vrf() -> Option<C::Hash> {
        Some(C::Hashing::hash(&[]))
    }
}

/// Build genesis storage according to the mock runtime.
// Linter complains even though it's not true, it's used in the benchmarks
#[allow(unused)]
pub fn new_test_ext() -> sp_io::TestExternalities {
    let t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap()
        .into();
    sp_io::TestExternalities::new(t)
}

/// Run until a particular block.
pub fn run_to_block(n: u64) {
    while System::block_number() < n {
        if System::block_number() > 1 {
            System::on_finalize(System::block_number());
        }
        System::set_block_number(System::block_number() + 1);
        System::on_initialize(System::block_number());

        RandomnessModule::set_author_vrf(RawOrigin::None.into()).unwrap();
    }
}
