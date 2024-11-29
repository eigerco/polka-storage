use frame_support::{assert_err, assert_ok};
use primitives::pallets::Randomness;

use crate::{
    mock::{new_test_ext, run_to_block, BlockNumber, RandomnessModule, SeedAgeLimit, Test},
    Error,
};

#[test]
fn test_randomness_availability() {
    new_test_ext().execute_with(|| {
        let n_blocks = 500;
        run_to_block(n_blocks);

        // Iterate all blocks and check if the random seed is available
        for block_number in 0..=n_blocks {
            let seed = <RandomnessModule as Randomness<BlockNumber>>::get_randomness(block_number);

            // Seed on zero block should never be available
            if block_number == 0 {
                assert_err!(seed, Error::<Test>::SeedNotAvailable);
                continue;
            }

            // Check availability
            match block_number {
                // Seeds older than SeedAgeLimit should not be available. They
                // were cleaned up.
                block_number if block_number < n_blocks - SeedAgeLimit::get() => {
                    assert_err!(seed, Error::<Test>::SeedNotAvailable);
                }
                // Other seeds should be available
                _else => {
                    assert_ok!(seed);
                }
            }
        }
    });
}

#[test]
fn test_randomness_uniqueness() {
    new_test_ext().execute_with(|| {
        let n_blocks = 200;
        run_to_block(n_blocks);

        // Iterate all blocks and check if the seed is different from the one
        // before
        let mut previous_seed = None;
        for block_number in 0..=n_blocks {
            let Ok(current_seed) =
                <RandomnessModule as Randomness<BlockNumber>>::get_randomness(block_number)
            else {
                continue;
            };

            if let Some(previous_seed) = previous_seed {
                assert_ne!(previous_seed, current_seed);
            }

            previous_seed = Some(current_seed);
        }
    })
}
