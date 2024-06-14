use frame_support::{derive_impl, parameter_types, PalletId};
use frame_system as system;
use sp_runtime::BuildStorage;

use crate as pallet_market;

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

pub const ALICE: u64 = 0;
pub const BOB: u64 = 1;
pub const INITIAL_FUNDS: u64 = 100;

/// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap()
        .into();
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(ALICE, INITIAL_FUNDS), (BOB, INITIAL_FUNDS)],
    }
    .assimilate_storage(&mut t)
    .unwrap();

    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

pub fn events() -> Vec<RuntimeEvent> {
    let evt = System::events()
        .into_iter()
        .map(|evt| evt.event)
        .collect::<Vec<_>>();
    System::reset_events();
    evt
}
