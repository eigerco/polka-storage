use cid::Cid;
use codec::Encode;
use frame_support::{
    assert_ok, derive_impl, pallet_prelude::ConstU32, parameter_types, sp_runtime::BoundedVec,
    traits::Hooks, PalletId,
};
use frame_system::pallet_prelude::BlockNumberFor;
use multihash_codetable::{Code, MultihashDigest};
use pallet_market::{BalanceOf, ClientDealProposal, DealProposal, DealState};
use primitives_proofs::{
    DealId, RegisteredPoStProof, RegisteredSealProof, SectorId, SectorNumber, MAX_DEALS_PER_SECTOR,
};
use sp_core::{bounded_vec, Pair};
use sp_runtime::{
    traits::{ConstU64, IdentifyAccount, IdentityLookup, Verify},
    BuildStorage, MultiSignature, MultiSigner,
};

use crate::{self as pallet_storage_provider, pallet::CID_CODEC, sector::SectorPreCommitInfo};

mod pre_commit_sector;
mod prove_commit_sector;
mod state;
mod storage_provider_registration;

type Block = frame_system::mocking::MockBlock<Test>;
type BlockNumber = u64;

const MILLISECS_PER_BLOCK: u64 = 12000;
const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;
const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber);
const HOURS: BlockNumber = MINUTES * 60;
const DAYS: BlockNumber = HOURS * 24;
const YEARS: BlockNumber = DAYS * 365;

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

type AccountIdOf<Test> = <Test as frame_system::Config>::AccountId;

type DealProposalOf<Test> =
    DealProposal<<Test as frame_system::Config>::AccountId, BalanceOf<Test>, BlockNumberFor<Test>>;

type ClientDealProposalOf<Test> = ClientDealProposal<
    <Test as frame_system::Config>::AccountId,
    BalanceOf<Test>,
    BlockNumberFor<Test>,
    MultiSignature,
>;

const ALICE: &'static str = "//Alice";
const BOB: &'static str = "//Bob";

/// Initial funds of all accounts.
const INITIAL_FUNDS: u64 = 100;

// Build genesis storage according to the mock runtime.
fn new_test_ext() -> sp_io::TestExternalities {
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

fn events() -> Vec<RuntimeEvent> {
    let evt = System::events()
        .into_iter()
        .map(|evt| evt.event)
        .collect::<Vec<_>>();
    System::reset_events();
    evt
}

fn cid_of(data: &str) -> cid::Cid {
    Cid::new_v1(CID_CODEC, Code::Blake2b256.digest(data.as_bytes()))
}

fn sign(pair: &sp_core::sr25519::Pair, bytes: &[u8]) -> MultiSignature {
    MultiSignature::Sr25519(pair.sign(bytes))
}

fn sign_proposal(client: &str, proposal: DealProposalOf<Test>) -> ClientDealProposalOf<Test> {
    let alice_pair = key_pair(client);
    let client_signature = sign(&alice_pair, &Encode::encode(&proposal));
    ClientDealProposal {
        proposal,
        client_signature,
    }
}

fn key_pair(name: &str) -> sp_core::sr25519::Pair {
    sp_core::sr25519::Pair::from_string(name, None).unwrap()
}

fn account(name: &str) -> AccountIdOf<Test> {
    let user_pair = key_pair(name);
    let signer = MultiSigner::Sr25519(user_pair.public());
    signer.into_account()
}

/// Run until a particular block.
///
/// Stolen't from: <https://github.com/paritytech/polkadot-sdk/blob/7df94a469e02e1d553bd4050b0e91870d6a4c31b/substrate/frame/lottery/src/mock.rs#L87-L98>
fn run_to_block(n: u64) {
    while System::block_number() < n {
        if System::block_number() > 1 {
            StorageProvider::on_finalize(System::block_number());
            System::on_finalize(System::block_number());
        }

        System::set_block_number(System::block_number() + 1);
        System::on_initialize(System::block_number());
        StorageProvider::on_initialize(System::block_number());
    }
}

/// Register account as a provider.
fn register_storage_provider(account: AccountIdOf<Test>) {
    let peer_id = "storage_provider_1".as_bytes().to_vec();
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

struct SectorPreCommitInfoBuilder {
    seal_proof: RegisteredSealProof,
    sector_number: SectorNumber,
    sealed_cid: SectorId,
    deal_ids: BoundedVec<DealId, ConstU32<MAX_DEALS_PER_SECTOR>>,
    expiration: u64,
    unsealed_cid: SectorId,
}

impl Default for SectorPreCommitInfoBuilder {
    fn default() -> Self {
        Self {
            seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
            sector_number: 1,
            sealed_cid: cid_of("sealed_cid")
                .to_bytes()
                .try_into()
                .expect("hash is always 32 bytes"),
            deal_ids: bounded_vec![0, 1],
            expiration: YEARS,
            unsealed_cid: cid_of("unsealed_cid")
                .to_bytes()
                .try_into()
                .expect("hash is always 32 bytes"),
        }
    }
}

impl SectorPreCommitInfoBuilder {
    pub fn sector_number(mut self, sector_number: u64) -> Self {
        self.sector_number = sector_number;
        self
    }

    pub fn deals<I>(mut self, deal_ids: I) -> Self
    where
        I: IntoIterator<Item = DealId>,
    {
        let deal_ids_vec = deal_ids.into_iter().collect::<Vec<_>>();
        self.deal_ids = BoundedVec::try_from(deal_ids_vec).unwrap();
        self
    }

    pub fn sealed_cid(mut self, data: &str) -> Self {
        self.sealed_cid = cid_of(data)
            .to_bytes()
            .try_into()
            .expect("hash is always 32 bytes");
        self
    }

    pub fn unsealed_cid(mut self, data: &str) -> Self {
        self.unsealed_cid = cid_of(data)
            .to_bytes()
            .try_into()
            .expect("hash is always 32 bytes");
        self
    }

    pub fn expiration(mut self, expiration: u64) -> Self {
        self.expiration = expiration;
        self
    }

    pub fn build(self) -> SectorPreCommitInfo<u64> {
        SectorPreCommitInfo {
            seal_proof: self.seal_proof,
            sector_number: self.sector_number,
            sealed_cid: self.sealed_cid,
            deal_ids: self.deal_ids,
            expiration: self.expiration,
            unsealed_cid: self.unsealed_cid,
        }
    }
}

/// Builder to simplify writing complex tests of [`DealProposal`].
/// Exclusively uses [`Test`] for simplification purposes.
struct DealProposalBuilder {
    piece_cid: BoundedVec<u8, ConstU32<128>>,
    piece_size: u64,
    client: AccountIdOf<Test>,
    provider: AccountIdOf<Test>,
    label: BoundedVec<u8, ConstU32<128>>,
    start_block: u64,
    end_block: u64,
    storage_price_per_block: u64,
    provider_collateral: u64,
    state: DealState<u64>,
}

impl Default for DealProposalBuilder {
    fn default() -> Self {
        Self {
            piece_cid: cid_of("polka-storage-data")
                .to_bytes()
                .try_into()
                .expect("hash is always 32 bytes"),
            piece_size: 18,
            client: account(BOB),
            provider: account(ALICE),
            label: bounded_vec![0xb, 0xe, 0xe, 0xf],
            start_block: 100,
            end_block: 110,
            storage_price_per_block: 5,
            provider_collateral: 25,
            state: DealState::Published,
        }
    }
}

impl DealProposalBuilder {
    pub fn client(mut self, client: &str) -> Self {
        self.client = account(client);
        self
    }

    pub fn provider(mut self, provider: &str) -> Self {
        self.provider = account(provider);
        self
    }

    pub fn unsigned(self) -> DealProposalOf<Test> {
        DealProposalOf::<Test> {
            piece_cid: self.piece_cid,
            piece_size: self.piece_size,
            client: self.client,
            provider: self.provider,
            label: self.label,
            start_block: self.start_block,
            end_block: self.end_block,
            storage_price_per_block: self.storage_price_per_block,
            provider_collateral: self.provider_collateral,
            state: self.state,
        }
    }

    pub fn signed(self, by: &str) -> ClientDealProposalOf<Test> {
        let built = self.unsigned();
        let signed = sign_proposal(by, built);
        signed
    }
}
