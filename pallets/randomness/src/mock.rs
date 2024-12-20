use frame_support::{derive_impl, traits::OnFinalize};
use frame_system::{mocking::MockBlock, RawOrigin};
use sp_runtime::{
    traits::{Hash, Header},
    BuildStorage,
};

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
    #[runtime::pallet_index(1)]
    pub type RandomnessModule = crate;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = MockBlock<Test>;
    type Nonce = u64;
}

impl crate::Config for Test {
    type AuthorVrfGetter = DummyVrf<Self>;
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

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap()
        .into();
    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

/// Run until a particular block.
pub fn run_to_block(n: u64) {
    let mut parent_hash = System::parent_hash();

    while System::block_number() <= n {
        let block_number = System::block_number();

        if System::block_number() > 1 {
            let finalizing_block_number = block_number - 1;
            System::on_finalize(finalizing_block_number);
        }

        // It's ok under test
        RandomnessModule::set_author_vrf(RawOrigin::None.into()).unwrap();

        System::initialize(&block_number, &parent_hash, &Default::default());

        let header = System::finalize();
        parent_hash = header.hash();
        System::set_block_number(*header.number() + 1);
        System::set_parent_hash(parent_hash);
    }
}
