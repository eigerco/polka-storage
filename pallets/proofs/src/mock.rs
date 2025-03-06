use std::fs;

use bls12_381::Bls12;
use codec::Decode;
use frame_support::derive_impl;
use frame_system::mocking::MockBlock;
use polka_storage_proofs::VerifyingKey;
use sp_runtime::BuildStorage;
use std::collections::BTreeMap;

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

    let config = crate::GenesisConfig::<Test> {
        porep_keys: BTreeMap::from([(
            primitives::proofs::RegisteredSealProof::StackedDRG1GiBV1,
            load_key("../../examples/1GiB.porep.vk.scale"),
        )]),
        post_keys: BTreeMap::from([(
            primitives::proofs::RegisteredPoStProof::StackedDRGWindow1GiBV1,
            load_key("../../examples/1GiB.post.vk.scale"),
        )]),
        _config: Default::default(),
    };

    config.build_storage().unwrap().into()
}

fn load_key(path: &'static str) -> VerifyingKey<Bls12> {
    let vkey_bytes = fs::read(path).expect("key file at the location to be available");
    Decode::decode(&mut vkey_bytes.as_slice()).unwrap()
}
