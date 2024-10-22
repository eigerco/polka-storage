use crate::mock::{new_test_ext, run_to_block};
use primitives_proofs::Randomness;
use crate::mock::{BlockNumber, RandomnessModule};

#[test]
fn test_randomness_availability() {
    new_test_ext().execute_with(|| {
        let seed = <RandomnessModule as Randomness<BlockNumber>>::get_randomness(10);
        dbg!(seed);

        run_to_block(100);

        let seed = <RandomnessModule as Randomness<BlockNumber>>::get_randomness(10);
        dbg!(seed);
    });
}