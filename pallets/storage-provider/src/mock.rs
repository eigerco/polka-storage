use cid::Cid;
use codec::Encode;
use frame_support::{
    derive_impl, pallet_prelude::ConstU32, parameter_types, sp_runtime::BoundedVec, PalletId,
};
use frame_system::pallet_prelude::BlockNumberFor;
use multihash_codetable::{Code, MultihashDigest};
use pallet_market::{BalanceOf, ClientDealProposal, DealProposal};
use sp_core::Pair;
use sp_runtime::{
    traits::{ConstU64, IdentifyAccount, IdentityLookup, Verify},
    BuildStorage, MultiSignature, MultiSigner,
};

use crate::{self as pallet_storage_provider, pallet::CID_CODEC};

type Block = frame_system::mocking::MockBlock<Test>;

type BlockNumber = u64;

const MILLISECS_PER_BLOCK: u64 = 12000;
const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;
const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber);
const HOURS: BlockNumber = MINUTES * 60;
const DAYS: BlockNumber = HOURS * 24;
pub const YEARS: BlockNumber = DAYS * 365;

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        StorageProvider: pallet_storage_provider::pallet,
        Market: pallet_market,
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
    pub const MarketPalletId: PalletId = PalletId(*b"spMarket");
}

impl pallet_market::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type PalletId = MarketPalletId;
    type Currency = Balances;
    type OffchainSignature = Signature;
    type OffchainPublic = AccountPublic;
    type MaxDeals = ConstU32<32>;
    type BlocksPerDay = ConstU64<1>;
    type MinDealDuration = ConstU64<1>;
    type MaxDealDuration = ConstU64<30>;
    type MaxDealsPerBlock = ConstU32<32>;
}

parameter_types! {
    pub const WpostProvingPeriod: BlockNumber = DAYS;
    // Half an hour (=48 per day)
    // 30 * 60 = 30 minutes
    // SLOT_DURATION is in milliseconds thats why we / 1000
    pub const WpostChallengeWindow: BlockNumber = 30 * 60 / (SLOT_DURATION as BlockNumber / 1000);
    pub const MinSectorExpiration: BlockNumber = 180 * DAYS;
    pub const MaxSectorExpirationExtension: BlockNumber = 1278 * DAYS;
    pub const SectorMaximumLifetime: BlockNumber = YEARS * 5;
    pub const MaxProveCommitDuration: BlockNumber =  (30 * DAYS) + 150;
}

impl pallet_storage_provider::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type PeerId = BoundedVec<u8, ConstU32<256>>; // Arbitrary length
    type Currency = Balances;
    type Market = Market;
    type WPoStProvingPeriod = WpostProvingPeriod;
    type WPoStChallengeWindow = WpostChallengeWindow;
    type MinSectorExpiration = MinSectorExpiration;
    type MaxSectorExpirationExtension = MaxSectorExpirationExtension;
    type SectorMaximumLifetime = SectorMaximumLifetime;
    type MaxProveCommitDuration = MaxProveCommitDuration;
}

pub type AccountIdOf<Test> = <Test as frame_system::Config>::AccountId;

pub fn key_pair(name: &str) -> sp_core::sr25519::Pair {
    sp_core::sr25519::Pair::from_string(name, None).unwrap()
}

pub fn account(name: &str) -> AccountIdOf<Test> {
    let user_pair = key_pair(name);
    let signer = MultiSigner::Sr25519(user_pair.public());
    signer.into_account()
}

pub const ALICE: &'static str = "//Alice";
pub const BOB: &'static str = "//Bob";
pub const INITIAL_FUNDS: u64 = 100;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap()
        .into();
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![
            (account(ALICE), INITIAL_FUNDS),
            (account(BOB), INITIAL_FUNDS),
        ],
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

pub fn cid_of(data: &str) -> cid::Cid {
    Cid::new_v1(CID_CODEC, Code::Blake2b256.digest(data.as_bytes()))
}

pub(crate) type DealProposalOf<T> =
    DealProposal<<T as frame_system::Config>::AccountId, BalanceOf<T>, BlockNumberFor<T>>;

type ClientDealProposalOf<T> = ClientDealProposal<
    <T as frame_system::Config>::AccountId,
    BalanceOf<T>,
    BlockNumberFor<T>,
    MultiSignature,
>;

pub fn sign(pair: &sp_core::sr25519::Pair, bytes: &[u8]) -> MultiSignature {
    MultiSignature::Sr25519(pair.sign(bytes))
}

pub fn sign_proposal(client: &str, proposal: DealProposalOf<Test>) -> ClientDealProposalOf<Test> {
    let alice_pair = key_pair(client);
    let client_signature = sign(&alice_pair, &Encode::encode(&proposal));
    ClientDealProposal {
        proposal,
        client_signature,
    }
}
