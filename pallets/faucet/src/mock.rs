use frame_support::{derive_impl, parameter_types, traits::Hooks};
use frame_system::{self as system};
use sp_core::Pair;
use sp_runtime::{
    traits::{IdentifyAccount, IdentityLookup, Verify},
    AccountId32, BuildStorage, MultiSignature, MultiSigner,
};

use crate::{self as pallet_faucet, BalanceOf};

pub const ALICE: &'static str = "//Alice";

type Block = frame_system::mocking::MockBlock<Test>;
type BlockNumber = u64;

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        Faucet: pallet_faucet,
    }
);

pub type Signature = MultiSignature;
pub type AccountPublic = <Signature as Verify>::Signer;
pub type AccountId = <AccountPublic as IdentifyAccount>::AccountId;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
    type AccountData = pallet_balances::AccountData<u64>;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<Self::AccountId>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
    type AccountStore = System;
}

parameter_types! {
    pub const FaucetAmount: BalanceOf<Test> = 10_000_000_000_000;
    pub const FaucetDelay: BlockNumber = 1;
}

impl crate::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type FaucetAmount = FaucetAmount;
    type FaucetDelay = FaucetDelay;
}

pub fn key_pair(name: &str) -> sp_core::sr25519::Pair {
    sp_core::sr25519::Pair::from_string(name, None).unwrap()
}

pub fn account<T: frame_system::Config>(name: &str) -> AccountId32 {
    let user_pair = key_pair(name);
    let signer = MultiSigner::Sr25519(user_pair.public());
    signer.into_account()
}

pub fn events() -> Vec<RuntimeEvent> {
    let evt = System::events()
        .into_iter()
        .map(|evt| evt.event)
        .collect::<Vec<_>>();
    System::reset_events();
    evt
}

/// Run until a particular block.
///
/// Stolen't from: <https://github.com/paritytech/polkadot-sdk/blob/7df94a469e02e1d553bd4050b0e91870d6a4c31b/substrate/frame/lottery/src/mock.rs#L87-L98>
pub fn run_to_block(n: u64) {
    while System::block_number() < n {
        if System::block_number() > 1 {
            System::on_finalize(System::block_number());
        }

        System::set_block_number(System::block_number() + 1);
        System::on_initialize(System::block_number());
    }
}

/// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let _ = env_logger::try_init();
    let t = system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap()
        .into();

    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}
