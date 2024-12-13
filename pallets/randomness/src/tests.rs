use sp_runtime::traits::Hash;

use crate::mock::{new_test_ext, run_to_block, RandomnessModule, Test};

#[test]
fn test_no_randomness() {
    new_test_ext().execute_with(|| {
        assert_eq!(<RandomnessModule>::author_vrf(), Default::default());
    })
}

#[test]
fn test_randomness() {
    new_test_ext().execute_with(|| {
        run_to_block(1);
        assert_eq!(
            <RandomnessModule>::author_vrf(),
            <Test as frame_system::Config>::Hashing::hash(&[])
        );
    })
}

#[test]
fn test_history() {
    new_test_ext().execute_with(|| {
        run_to_block(256);
        assert_eq!(<RandomnessModule>::author_vrf_history(0), None);
        assert_eq!(
            <RandomnessModule>::author_vrf_history(1),
            Some(<Test as frame_system::Config>::Hashing::hash(&[]))
        );
        assert_eq!(<RandomnessModule>::author_vrf_history(258), None);
    })
}
