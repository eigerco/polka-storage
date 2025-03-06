use alloc::collections::BTreeMap;
use codec::Decode;
use frame_support::derive_impl;
use frame_system::mocking::MockBlock;
use sp_runtime::BuildStorage;

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
    pub type ProofsModule = crate;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = MockBlock<Test>;
    type Nonce = u64;
}

impl crate::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let _ = env_logger::try_init();

    let mut post_keys = BTreeMap::new();
    let vkey_bytes = include_bytes!("../../../examples/1GiB.post.vk.scale").to_vec();
    let vkey = Decode::decode(&mut vkey_bytes.as_slice()).unwrap();
    post_keys.insert(primitives::proofs::RegisteredPoStProof::StackedDRGWindow1GiBV1, vkey);
    
    let config = crate::GenesisConfig::<Test> {
        porep_keys: BTreeMap::new(),
        post_keys,
        _config: Default::default(),
    };

    config
        .build_storage()
        .unwrap()
        .into()
}
