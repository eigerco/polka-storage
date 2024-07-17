use super::new_test_ext;
use crate::{
    pallet::StorageProviders,
    tests::{account, Test, ALICE, BOB},
};

#[test]
fn initial_state() {
    new_test_ext().execute_with(|| {
        assert!(!StorageProviders::<Test>::contains_key(account(ALICE)));
        assert!(!StorageProviders::<Test>::contains_key(account(BOB)));
    })
}
