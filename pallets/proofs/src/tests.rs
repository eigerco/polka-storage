use frame_support::assert_ok;

use crate::mock::*;

#[test]
fn it_works_for_default_value() {
    new_test_ext().execute_with(|| {
        assert_ok!(ProofsModule::do_something(RuntimeOrigin::signed(1), 42));
    });
}
