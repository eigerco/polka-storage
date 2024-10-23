use crate::mock::{new_test_ext, run_to_block};
use primitives_proofs::Randomness;
use crate::mock::{BlockNumber, RandomnessModule};
use frame_support::{assert_err, assert_ok};
use crate::mock::Test;
use crate::Error;

#[test]
fn test_randomness_availability() {
    new_test_ext().execute_with(|| {
        let n_blocks = 100;
        run_to_block(n_blocks);

        // Iterate all blocks and check if the random seed is available
        for block_number in 0..=n_blocks {
            let seed = <RandomnessModule as Randomness<BlockNumber>>::get_randomness(block_number);

            // Seed on zero block should not be available
            if block_number == 0 {
                assert_err!(seed, Error::<Test>::SeedNotAvailable);
                continue;
            }

            // Seeds for the last 81 blocks should not be available. That will
            // probably change when we change the underlying randomness
            // algorithm.
            if block_number > n_blocks - (81 + 1) {
                assert_err!(seed, Error::<Test>::SeedNotAvailable);
            } else {
                assert_ok!(seed);
            }
        }
    });
}