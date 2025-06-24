#![cfg(feature = "runtime-benchmarks")]

use frame_benchmarking::v2::*;
use frame_system::RawOrigin;

use super::*;
#[allow(unused)]
use crate::Pallet as ProofsPallet;

#[frame_benchmarking::v2::benchmarks(
    where T: crate::Config
)]
mod benchmarks {
    use primitives::proofs::{RegisteredPoStProof, RegisteredSealProof};
    use rand::SeedableRng;
    use rand_xorshift::XorShiftRng;

    use super::*;
    use crate::crypto::groth16::{Bls12, VerifyingKey};

    // Copies from the tests module to simplify imports, etc
    const TEST_SEED: [u8; 16] = [
        0x59, 0x62, 0xbe, 0x5d, 0x76, 0x3d, 0x31, 0x8d, 0x17, 0xdb, 0x37, 0x32, 0x54, 0x06, 0xbc,
        0xe5,
    ];

    #[benchmark]
    fn set_porep_verifying_key() {
        let mut rng = XorShiftRng::from_seed(TEST_SEED);
        let vkey = VerifyingKey::<Bls12>::random(&mut rng);
        let mut vkey_bytes = vec![0u8; vkey.serialised_bytes()];
        vkey.clone()
            .into_bytes(&mut vkey_bytes.as_mut_slice())
            .unwrap();

        #[extrinsic_call]
        _(
            RawOrigin::Root,
            RegisteredSealProof::StackedDRG2KiBV1P1,
            vkey_bytes,
        );

        assert!(PoRepVerifyingKeys::<T>::get(&RegisteredSealProof::StackedDRG2KiBV1P1).is_some());
    }

    #[benchmark]
    fn set_post_verifying_key() {
        let mut rng = XorShiftRng::from_seed(TEST_SEED);
        let vkey = VerifyingKey::<Bls12>::random(&mut rng);
        let mut vkey_bytes = vec![0u8; vkey.serialised_bytes()];
        vkey.clone()
            .into_bytes(&mut vkey_bytes.as_mut_slice())
            .unwrap();

        #[extrinsic_call]
        _(
            RawOrigin::Root,
            RegisteredPoStProof::StackedDRGWindow2KiBV1P1,
            vkey_bytes,
        );

        assert!(
            PoStVerifyingKeys::<T>::get(&RegisteredPoStProof::StackedDRGWindow2KiBV1P1).is_some()
        );
    }

    impl_benchmark_test_suite! {
        ProofsPallet,
        crate::mock::new_test_ext(),
        crate::mock::Test,
    }
}
