// TODO(@Jinxit,19/03/2025): Remove this cfg if tests are moved here from pallet-storage-provider.
#![cfg(feature = "runtime-benchmarks")]

use std::sync::Arc;

use sp_keystore::{testing::MemoryKeystore, KeystoreExt};
use sp_runtime::BuildStorage;

use crate::benchmarks::mock::Test;

pub fn new_test_ext() -> sp_io::TestExternalities {
    let _ = env_logger::try_init();
    let t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap()
        .into();

    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| frame_system::Pallet::<Test>::set_block_number(1));

    // Required to perform signatures. Given that benchmarks run inside the runtime, this is how
    // we're able to prepare signed client deal proposals.
    let keystore = MemoryKeystore::new();
    ext.register_extension(KeystoreExt(Arc::new(keystore)));

    ext
}
