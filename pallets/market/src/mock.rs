use cid::Cid;
use codec::Encode;
use frame_support::{
    assert_ok, derive_impl, parameter_types,
    sp_runtime::BoundedVec,
    traits::{OnFinalize, OnInitialize},
    PalletId,
};
use frame_system::{self as system, pallet_prelude::BlockNumberFor};
use multihash_codetable::{Code, MultihashDigest};
use primitives_proofs::RegisteredPoStProof;
use sp_core::Pair;
use sp_runtime::{
    traits::{ConstU32, ConstU64, IdentifyAccount, IdentityLookup, Verify},
    AccountId32, BuildStorage, MultiSignature, MultiSigner,
};

use crate::{self as pallet_market, BalanceOf, ClientDealProposal, DealProposal, CID_CODEC};

type Block = frame_system::mocking::MockBlock<Test>;
type BlockNumber = u64;

const MINUTES: BlockNumber = 1;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
    pub enum Test
    {
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
impl system::Config for Test {
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
    // Market Pallet
    pub const MarketPalletId: PalletId = PalletId(*b"spMarket");

    // Storage Provider Pallet
    pub const WPoStPeriodDeadlines: u64 = 10;
    pub const WpostProvingPeriod: BlockNumber = 40 * MINUTES;
    pub const WpostChallengeWindow: BlockNumber = 4 * MINUTES;
    pub const WpostChallengeLookBack: BlockNumber = MINUTES;
    pub const MinSectorExpiration: BlockNumber = 5 * MINUTES;
    pub const MaxSectorExpirationExtension: BlockNumber = 360 * MINUTES;
    pub const SectorMaximumLifetime: BlockNumber = 120 * MINUTES;
    pub const MaxProveCommitDuration: BlockNumber = 5 * MINUTES;
    pub const MaxPartitionsPerDeadline: u64 = 3000;
    pub const FaultMaxAge: BlockNumber = (5 * MINUTES) * 42;
    pub const FaultDeclarationCutoff: BlockNumber = 2 * MINUTES;
}

impl crate::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type PalletId = MarketPalletId;
    type Currency = Balances;
    type OffchainSignature = Signature;
    type OffchainPublic = AccountPublic;
    type StorageProvider = StorageProvider;
    type MaxDeals = ConstU32<32>;
    type MinDealDuration = ConstU64<2>;
    type MaxDealDuration = ConstU64<30>;
    type MaxDealsPerBlock = ConstU32<32>;
}

impl pallet_storage_provider::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type PeerId = BoundedVec<u8, ConstU32<256>>; // Arbitrary length
    type Currency = Balances;
    type Market = Market;
    type WPoStProvingPeriod = WpostProvingPeriod;
    type WPoStChallengeWindow = WpostChallengeWindow;
    type WPoStChallengeLookBack = WpostChallengeLookBack;
    type MinSectorExpiration = MinSectorExpiration;
    type MaxSectorExpirationExtension = MaxSectorExpirationExtension;
    type SectorMaximumLifetime = SectorMaximumLifetime;
    type MaxProveCommitDuration = MaxProveCommitDuration;
    type WPoStPeriodDeadlines = WPoStPeriodDeadlines;
    type MaxPartitionsPerDeadline = MaxPartitionsPerDeadline;
    type FaultMaxAge = FaultMaxAge;
    type FaultDeclarationCutoff = FaultDeclarationCutoff;
}

pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

pub fn key_pair(name: &str) -> sp_core::sr25519::Pair {
    sp_core::sr25519::Pair::from_string(name, None).unwrap()
}

pub fn account<T: frame_system::Config>(name: &str) -> AccountId32 {
    let user_pair = key_pair(name);
    let signer = MultiSigner::Sr25519(user_pair.public());
    signer.into_account()
}

pub fn sign(pair: &sp_core::sr25519::Pair, bytes: &[u8]) -> MultiSignature {
    MultiSignature::Sr25519(pair.sign(bytes))
}

pub fn cid_of(data: &str) -> cid::Cid {
    Cid::new_v1(CID_CODEC, Code::Blake2b256.digest(data.as_bytes()))
}

pub(crate) type DealProposalOf<T> =
    DealProposal<<T as frame_system::Config>::AccountId, BalanceOf<T>, BlockNumberFor<T>>;

pub(crate) type ClientDealProposalOf<T> = ClientDealProposal<
    <T as frame_system::Config>::AccountId,
    BalanceOf<T>,
    BlockNumberFor<T>,
    MultiSignature,
>;

pub fn sign_proposal(client: &str, proposal: DealProposalOf<Test>) -> ClientDealProposalOf<Test> {
    let alice_pair = key_pair(client);
    let client_signature = sign(&alice_pair, &Encode::encode(&proposal));
    ClientDealProposal {
        proposal,
        client_signature,
    }
}

pub const ALICE: &'static str = "//Alice";
pub const BOB: &'static str = "//Bob";
pub const PROVIDER: &'static str = "//StorageProvider";
pub const INITIAL_FUNDS: u64 = 100;

/// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let _ = env_logger::try_init();
    let mut t = system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap()
        .into();
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![
            (account::<Test>(ALICE), INITIAL_FUNDS),
            (account::<Test>(BOB), INITIAL_FUNDS),
            (account::<Test>(PROVIDER), INITIAL_FUNDS),
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

/// Run until a particular block.
///
/// Stolen't from: <https://github.com/paritytech/polkadot-sdk/blob/7df94a469e02e1d553bd4050b0e91870d6a4c31b/substrate/frame/lottery/src/mock.rs#L87-L98>
pub fn run_to_block(n: u64) {
    while System::block_number() < n {
        if System::block_number() > 1 {
            Market::on_finalize(System::block_number());
            System::on_finalize(System::block_number());
        }
        System::set_block_number(System::block_number() + 1);
        System::on_initialize(System::block_number());
        Market::on_initialize(System::block_number());
    }
}

/// Register account as a provider.
pub(crate) fn register_storage_provider(account: AccountIdOf<Test>) {
    let peer_id: Vec<u8> = "storage_provider_1".as_bytes().to_vec();
    let peer_id = BoundedVec::try_from(peer_id).unwrap();
    let window_post_type = RegisteredPoStProof::StackedDRGWindow2KiBV1P1;

    // Register account as a storage provider.
    assert_ok!(StorageProvider::register_storage_provider(
        RuntimeOrigin::signed(account),
        peer_id.clone(),
        window_post_type,
    ));

    // Remove any events that were triggered during registration.
    System::reset_events();
}
