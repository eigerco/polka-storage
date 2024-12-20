extern crate alloc;
use alloc::collections::BTreeSet;
use core::str::FromStr;

use cid::Cid;
use codec::Encode;
use frame_support::{
    assert_ok, derive_impl,
    pallet_prelude::ConstU32,
    parameter_types,
    sp_runtime::{BoundedBTreeMap, BoundedVec},
    traits::Hooks,
    PalletId,
};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_market::{BalanceOf, ClientDealProposal, DealProposal, DealState};
use primitives::{
    commitment::{CommP, Commitment, RawCommitment},
    pallets::ProofVerification,
    proofs::{ProverId, PublicReplicaInfo, RegisteredPoStProof, RegisteredSealProof, Ticket},
    sector::SectorNumber,
    DealId, PartitionNumber, CID_SIZE_IN_BYTES, MAX_DEALS_PER_SECTOR, MAX_PARTITIONS_PER_DEADLINE,
    MAX_POST_PROOF_BYTES, MAX_SEAL_PROOF_BYTES, MAX_SECTORS_PER_PROOF, MAX_TERMINATIONS_PER_CALL,
};
use sp_arithmetic::traits::Zero;
use sp_core::{bounded_vec, Pair};
use sp_runtime::{
    traits::{IdentifyAccount, IdentityLookup, Verify},
    BoundedBTreeSet, BuildStorage, MultiSignature, MultiSigner,
};

use crate::{
    self as pallet_storage_provider,
    fault::{
        DeclareFaultsParams, DeclareFaultsRecoveredParams, FaultDeclaration, RecoveryDeclaration,
    },
    pallet::DECLARATIONS_MAX,
    proofs::{PoStProof, SubmitWindowedPoStParams},
    sector::SectorPreCommitInfo,
};

mod deadline;
mod declare_faults;
mod declare_faults_recovered;
mod expiration_queue;
mod post_hook;
mod pre_commit_sector_hook;
mod pre_commit_sectors;
mod prove_commit_sectors;
mod state;
mod storage_provider_registration;
mod submit_windowed_post;
mod terminate_sectors;

type Block = frame_system::mocking::MockBlock<Test>;
type BlockNumber = u64;

const MINUTES: BlockNumber = 1;

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

pub const INVALID_PROOF: [u8; 2] = [0xd, 0xe];

/// This is dummy proofs pallet implementation. All proofs are accepted as valid
pub struct DummyProofsVerification;
impl ProofVerification for DummyProofsVerification {
    fn verify_porep(
        _prover_id: ProverId,
        _seal_proof: RegisteredSealProof,
        _comm_r: RawCommitment,
        _comm_d: RawCommitment,
        _sector: SectorNumber,
        _ticket: Ticket,
        _seed: Ticket,
        _proof: BoundedVec<u8, ConstU32<MAX_SEAL_PROOF_BYTES>>,
    ) -> sp_runtime::DispatchResult {
        Ok(())
    }

    fn verify_post(
        _post_type: RegisteredPoStProof,
        _randomness: Ticket,
        _replicas: BoundedBTreeMap<
            SectorNumber,
            PublicReplicaInfo,
            ConstU32<MAX_SECTORS_PER_PROOF>,
        >,
        proof: BoundedVec<u8, ConstU32<MAX_POST_PROOF_BYTES>>,
    ) -> sp_runtime::DispatchResult {
        if *proof == INVALID_PROOF {
            return Err(sp_runtime::DispatchError::Other("invalid proof"));
        }
        Ok(())
    }
}

impl pallet_market::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type PalletId = MarketPalletId;
    type Currency = Balances;
    type OffchainSignature = Signature;
    type OffchainPublic = AccountPublic;
    type StorageProviderValidation = StorageProvider;
    type MaxDeals = ConstU32<500>;
    type MinDealDuration = MinDealDuration;
    type MaxDealDuration = MaxDealDuration;
    type MaxDealsPerBlock = ConstU32<500>;
}

parameter_types! {
    // Storage Provider Pallet
    pub const WPoStPeriodDeadlines: u64 = 10;
    pub const WPoStProvingPeriod: BlockNumber = 40 * MINUTES;
    pub const WPoStChallengeWindow: BlockNumber = 4 * MINUTES;
    pub const WPoStChallengeLookBack: BlockNumber = MINUTES;
    pub const MinSectorExpiration: BlockNumber = 5 * MINUTES;
    pub const MaxSectorExpiration: BlockNumber = 360 * MINUTES;
    pub const SectorMaximumLifetime: BlockNumber = 120 * MINUTES;
    pub const MaxProveCommitDuration: BlockNumber = 5 * MINUTES;
    pub const MaxPartitionsPerDeadline: u64 = 3000;
    pub const FaultMaxAge: BlockNumber = (5 * MINUTES) * 42;
    pub const FaultDeclarationCutoff: BlockNumber = 2 * MINUTES;
    // 0 allows us to publish the prove-commit on the same block as the
    // pre-commit.
    pub const PreCommitChallengeDelay: BlockNumber = 0;
    // <https://github.com/filecoin-project/builtin-actors/blob/8d957d2901c0f2044417c268f0511324f591cb92/runtime/src/runtime/policy.rs#L299>
    pub const AddressedSectorsMax: u64 = 25_000;

    // Market Pallet
    pub const MarketPalletId: PalletId = PalletId(*b"spMarket");
    pub const MinDealDuration: u64 = 2 * MINUTES;
    pub const MaxDealDuration: u64 = 30 * MINUTES;
}

/// Randomness generator used by tests.
pub struct DummyRandomnessGenerator<C>(core::marker::PhantomData<C>)
where
    C: frame_system::Config;

impl<C> frame_support::traits::Randomness<C::Hash, BlockNumberFor<C>>
    for DummyRandomnessGenerator<C>
where
    C: frame_system::Config,
{
    fn random(_subject: &[u8]) -> (C::Hash, BlockNumberFor<C>) {
        (
            Default::default(),
            <frame_system::Pallet<C>>::block_number(),
        )
    }
}

impl<C> primitives::randomness::AuthorVrfHistory<BlockNumberFor<C>, C::Hash>
    for DummyRandomnessGenerator<C>
where
    C: frame_system::Config,
{
    fn author_vrf_history(block_number: BlockNumberFor<C>) -> Option<C::Hash> {
        if block_number == <BlockNumberFor<C> as Zero>::zero() {
            None
        } else {
            Some(Default::default())
        }
    }
}

impl pallet_storage_provider::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Randomness = DummyRandomnessGenerator<Self>;
    type AuthorVrfHistory = DummyRandomnessGenerator<Self>;
    type PeerId = BoundedVec<u8, ConstU32<32>>; // Max length of SHA256 hash
    type Currency = Balances;
    type Market = Market;
    type ProofVerification = DummyProofsVerification;
    type WPoStProvingPeriod = WPoStProvingPeriod;
    type WPoStChallengeWindow = WPoStChallengeWindow;
    type WPoStChallengeLookBack = WPoStChallengeLookBack;
    type MinSectorExpiration = MinSectorExpiration;
    type MaxSectorExpiration = MaxSectorExpiration;
    type SectorMaximumLifetime = SectorMaximumLifetime;
    type MaxProveCommitDuration = MaxProveCommitDuration;
    type WPoStPeriodDeadlines = WPoStPeriodDeadlines;
    type MaxPartitionsPerDeadline = MaxPartitionsPerDeadline;
    type FaultMaxAge = FaultMaxAge;
    type FaultDeclarationCutoff = FaultDeclarationCutoff;
    type PreCommitChallengeDelay = PreCommitChallengeDelay;
    // <https://github.com/filecoin-project/builtin-actors/blob/8d957d2901c0f2044417c268f0511324f591cb92/runtime/src/runtime/policy.rs#L295>
    type AddressedPartitionsMax = MaxPartitionsPerDeadline;
    type AddressedSectorsMax = AddressedSectorsMax;
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
const CHARLIE: &'static str = "//Charlie";

/// Initial funds of all accounts.
const INITIAL_FUNDS: u64 = 50000;

// Build genesis storage according to the mock runtime.
fn new_test_ext() -> sp_io::TestExternalities {
    let _ = env_logger::try_init();
    let mut t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap()
        .into();

    pallet_balances::GenesisConfig::<Test> {
        balances: vec![
            (account(ALICE), INITIAL_FUNDS),
            (account(BOB), INITIAL_FUNDS),
            (account(CHARLIE), INITIAL_FUNDS),
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
pub fn run_to_block(n: u64) {
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

/// This is a helper function to easily create a set of sectors.
pub fn sector_set<const B: u32>(sectors: &[u32]) -> BoundedBTreeSet<SectorNumber, ConstU32<B>> {
    let sectors = sectors
        .into_iter()
        .copied()
        .map(|s| s.try_into().unwrap())
        .collect::<BTreeSet<_>>();
    BoundedBTreeSet::try_from(sectors).unwrap()
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

/// Publish deals to Market Pallet for the sectors to be properly pre-committed and proven.
/// Pre-commit requires it as it calls [`Market::verify_deals_for_activation`].
///
/// It adds balance to the Market Pallet and publishes 2 deals to match default values in [`SectorPreCommitInfoBuilder`].
/// It also resets events to not interfere with [`events()`] assertions.
/// Deal 1: Client = Alice, Provider = provided
/// Deal 2: Client = Bob, Provider = provided
/// Balances: Alice = 60, Bob = 70, Provider = 70
fn publish_deals(storage_provider: &str) {
    // Add balance to the market pallet
    assert_ok!(Market::add_balance(
        RuntimeOrigin::signed(account(ALICE)),
        60
    ));
    assert_ok!(Market::add_balance(RuntimeOrigin::signed(account(BOB)), 60));
    assert_ok!(Market::add_balance(
        RuntimeOrigin::signed(account(storage_provider)),
        70
    ));

    // Publish the deal proposal
    Market::publish_storage_deals(
        RuntimeOrigin::signed(account(storage_provider)),
        bounded_vec![
            DealProposalBuilder::default()
                .client(ALICE)
                .provider(storage_provider)
                .signed(ALICE),
            DealProposalBuilder::default()
                .client(BOB)
                .provider(storage_provider)
                .signed(BOB)
        ],
    )
    .expect("publish_storage_deals needs to work in order to call verify_deals_for_activation");
    System::reset_events();
}

struct SectorPreCommitInfoBuilder {
    seal_proof: RegisteredSealProof,
    sector_number: SectorNumber,
    sealed_cid: BoundedVec<u8, ConstU32<CID_SIZE_IN_BYTES>>,
    deal_ids: BoundedVec<DealId, ConstU32<MAX_DEALS_PER_SECTOR>>,
    expiration: u64,
    unsealed_cid: BoundedVec<u8, ConstU32<CID_SIZE_IN_BYTES>>,
    seal_randomness_height: u64,
}

impl Default for SectorPreCommitInfoBuilder {
    fn default() -> Self {
        let unsealed_cid =
            Cid::from_str("baga6ea4seaqmruupwrxaeck7m3f5jtswpr7jv6bvwqeu5jinzjlcybh6er3ficq")
                .unwrap()
                .to_bytes()
                .try_into()
                .expect("hash is always 32 bytes");

        let sealed_cid =
            Cid::from_str("bagboea4b5abcamxmh7exq7vrvacvajooeapagr3a4g3tpjhw73iny47hvafw76gr")
                .unwrap()
                .to_bytes()
                .try_into()
                .expect("hash is always 32 bytes");

        Self {
            seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
            sector_number: SectorNumber::new(1).unwrap(),
            sealed_cid,
            deal_ids: bounded_vec![0, 1],
            expiration: 120 * MINUTES,
            unsealed_cid,
            seal_randomness_height: 1,
        }
    }
}

impl SectorPreCommitInfoBuilder {
    pub fn sector_number(mut self, sector_number: SectorNumber) -> Self {
        self.sector_number = sector_number;
        self
    }

    pub fn deals(mut self, deal_ids: Vec<u64>) -> Self {
        self.deal_ids = BoundedVec::try_from(deal_ids).unwrap();
        self
    }

    pub fn expiration(mut self, expiration: u64) -> Self {
        self.expiration = expiration;
        self
    }

    pub fn unsealed_cid(mut self, unsealed_cid: &str) -> Self {
        let cid = Cid::from_str(unsealed_cid).expect("valid unsealed_cid");
        self.unsealed_cid = BoundedVec::try_from(cid.to_bytes()).unwrap();
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
            seal_randomness_height: self.seal_randomness_height,
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
        let piece_commitment = Commitment::<CommP>::from(*b"dummydummydummydummydummydummydu");

        Self {
            piece_cid: piece_commitment
                .cid()
                .to_bytes()
                .try_into()
                .expect("hash is always 32 bytes"),
            piece_size: 128, // Smallest piece size available for sector
            client: account(BOB),
            provider: account(ALICE),
            label: bounded_vec![0xb, 0xe, 0xe, 0xf],
            start_block: 100 * MINUTES,
            end_block: 110 * MINUTES,
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

    pub fn label(mut self, label: Vec<u8>) -> Self {
        self.label = BoundedVec::try_from(label).unwrap();
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

struct SubmitWindowedPoStBuilder {
    deadline: u64,
    partitions: BoundedVec<PartitionNumber, ConstU32<MAX_PARTITIONS_PER_DEADLINE>>,
    proof: PoStProof,
}

impl SubmitWindowedPoStBuilder {
    pub fn deadline(mut self, deadline: u64) -> Self {
        self.deadline = deadline;
        self
    }

    pub fn partition(mut self, partition: PartitionNumber) -> Self {
        self.partitions = bounded_vec![partition];
        self
    }

    pub fn partitions(
        mut self,
        partitions: BoundedVec<PartitionNumber, ConstU32<MAX_PARTITIONS_PER_DEADLINE>>,
    ) -> Self {
        self.partitions = partitions;
        self
    }

    pub fn proof_bytes(mut self, proof_bytes: Vec<u8>) -> Self {
        self.proof.proof_bytes = BoundedVec::try_from(proof_bytes).unwrap();
        self
    }

    pub fn build(self) -> SubmitWindowedPoStParams {
        SubmitWindowedPoStParams {
            deadline: self.deadline,
            partitions: self.partitions,
            proof: self.proof,
        }
    }
}

impl Default for SubmitWindowedPoStBuilder {
    fn default() -> Self {
        Self {
            deadline: 0,
            partitions: bounded_vec![1],
            proof: PoStProof {
                post_proof: RegisteredPoStProof::StackedDRGWindow2KiBV1P1,
                proof_bytes: bounded_vec![0x1, 0x2, 0x3],
            },
        }
    }
}

struct DeclareFaultsBuilder {
    pub faults: BoundedVec<FaultDeclaration, ConstU32<DECLARATIONS_MAX>>,
}

impl Default for DeclareFaultsBuilder {
    fn default() -> Self {
        Self {
            faults: bounded_vec![],
        }
    }
}

impl DeclareFaultsBuilder {
    /// Build a fault declaration for a single deadline and partition.
    /// Multiple sector numbers can be passed in.
    pub fn fault(
        mut self,
        deadline: u64,
        partition: PartitionNumber,
        sectors: &[SectorNumber],
    ) -> Self {
        let fault_sectors: BoundedBTreeSet<SectorNumber, ConstU32<MAX_TERMINATIONS_PER_CALL>> =
            sectors
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .try_into()
                .expect(&format!(
                    "Could not convert a Vec with length {} to a BoundedBTreeSet with length {}",
                    sectors.len(),
                    MAX_TERMINATIONS_PER_CALL
                ));
        let fault = FaultDeclaration {
            deadline,
            partition,
            sectors: fault_sectors,
        };

        self.faults.try_push(fault).unwrap();
        self
    }

    pub fn build(self) -> DeclareFaultsParams {
        DeclareFaultsParams {
            faults: self.faults,
        }
    }
}

struct DeclareFaultsRecoveredBuilder {
    recoveries: BoundedVec<RecoveryDeclaration, ConstU32<DECLARATIONS_MAX>>,
}

impl Default for DeclareFaultsRecoveredBuilder {
    fn default() -> Self {
        Self {
            recoveries: bounded_vec![],
        }
    }
}

impl DeclareFaultsRecoveredBuilder {
    /// Build a fault recovery for a single deadline and partition.
    /// Multiple sector numbers can be passed in.
    pub fn fault_recovery(
        mut self,
        deadline: u64,
        partition: PartitionNumber,
        sectors: &[SectorNumber],
    ) -> Self {
        let recovered_sectors: BoundedBTreeSet<SectorNumber, ConstU32<MAX_TERMINATIONS_PER_CALL>> =
            sectors
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .try_into()
                .expect(&format!(
                    "Could not convert a Vec with length {} to a BoundedBTreeSet with length {}",
                    sectors.len(),
                    MAX_TERMINATIONS_PER_CALL
                ));
        let recovery = RecoveryDeclaration {
            deadline,
            partition,
            sectors: recovered_sectors,
        };

        self.recoveries.try_push(recovery).unwrap();
        self
    }

    pub fn build(self) -> DeclareFaultsRecoveredParams {
        DeclareFaultsRecoveredParams {
            recoveries: self.recoveries,
        }
    }
}
