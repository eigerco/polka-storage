use crate as pallet_market;
use frame_support::{derive_impl, parameter_types, PalletId};
use frame_system as system;
use sp_runtime::BuildStorage;

type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
        Balances: pallet_balances,
        Market: pallet_market,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl system::Config for Test {
	type Block = Block;
    type AccountData = pallet_balances::AccountData<u64>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
    type AccountStore = System;
}

parameter_types! {
    pub const MarketPalletId: PalletId = PalletId(*b"spMarket");
}

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
    type PalletId = MarketPalletId;
    type Currency = Balances;
}

/// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	system::GenesisConfig::<Test>::default().build_storage().unwrap().into()
}