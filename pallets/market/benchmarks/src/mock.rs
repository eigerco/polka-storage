extern crate alloc;

use frame_support::{derive_impl, parameter_types, sp_runtime::BoundedVec, PalletId};
use frame_system::pallet_prelude::BlockNumberFor;
use primitives::PEER_ID_MAX_BYTES;
use sp_runtime::{
    traits::{ConstU32, IdentifyAccount, IdentityLookup, Verify, Zero},
    MultiSignature,
};

type Block = frame_system::mocking::MockBlock<Test>;
type BlockNumber = u64;

const MINUTES: BlockNumber = 10;

#[frame_support::runtime]
mod runtime {
    #[runtime::runtime]
    #[runtime::derive(
        RuntimeCall,
        RuntimeEvent,
        RuntimeError,
        RuntimeOrigin,
        RuntimeFreezeReason,
        RuntimeHoldReason,
        RuntimeSlashReason,
        RuntimeLockId,
        RuntimeTask
    )]
    pub struct Test;

    #[runtime::pallet_index(0)]
    pub type System = frame_system::Pallet<Test>;

    #[runtime::pallet_index(10)]
    pub type Balances = pallet_balances::Pallet<Test>;

    #[runtime::pallet_index(34)]
    pub type StorageProvider = pallet_storage_provider::Pallet<Test>;

    #[runtime::pallet_index(35)]
    pub type Market = pallet_market::Pallet<Test>;

    #[runtime::pallet_index(36)]
    pub type Proofs = pallet_proofs::Pallet<Test>;
}

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

// NOTE(@jmg-duarte,20/01/2025): this is not ideal, however, in the name of time this is A solution
// the test parameters SHOULD be in sync with the parameters for the testnet configuration
// otherwise, it's impossible to run benchmarks on the node (to get weights)
// this BREAKS the normal tests when running benchmarks, as such you MUST run them in isolation
// cargo t -p pallet-market -F runtime-benchmarks -- bench
#[cfg(not(feature = "runtime-benchmarks"))]
parameter_types! {
    pub const MinDealDuration: u64 = 2;
    pub const MaxDealDuration: u64 = 30;
    // 0 allows us to publish the prove-commit on the same block as the
    // pre-commit.
    pub const PreCommitChallengeDelay: BlockNumber = 0;
}

#[cfg(feature = "runtime-benchmarks")]
parameter_types! {
    pub const MinDealDuration: u64 = 5 * MINUTES;
    pub const MaxDealDuration: u64 = 180 * MINUTES;
    pub const PreCommitChallengeDelay: BlockNumber = 1 * MINUTES;
}

impl pallet_market::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type PalletId = MarketPalletId;
    type WeightInfo = ();

    type Currency = Balances;
    type OffchainSignature = Signature;
    type OffchainPublic = AccountPublic;
    type StorageProviderValidation = StorageProvider;
    type MaxDeals = ConstU32<32>;
    type MinDealDuration = MinDealDuration;
    type MaxDealDuration = MaxDealDuration;
    type MaxDealsPerBlock = ConstU32<32>;
}

parameter_types! {
    // Market Pallet
    pub const MarketPalletId: PalletId = PalletId(*b"spMarket");

    // Storage Provider Pallet
    pub const WPostProvingPeriod: BlockNumber = 6 * MINUTES;
    pub const WPoStPeriodDeadlines: u64 = 3;
    pub const WPostChallengeWindow: BlockNumber = 2 * MINUTES;
    pub const WPostChallengeLookBack: BlockNumber = MINUTES;
    pub const MinSectorExpiration: BlockNumber = 5 * MINUTES;
    pub const MaxSectorExpiration: BlockNumber = 60 * MINUTES;
    pub const SectorMaximumLifetime: BlockNumber = 120 * MINUTES;
    pub const MaxProveCommitDuration: BlockNumber = 5 * MINUTES;
    pub const MaxPartitionsPerDeadline: u64 = 3000;
    pub const FaultMaxAge: BlockNumber = (5 * MINUTES) * 42;
    pub const FaultDeclarationCutoff: BlockNumber = 1 * MINUTES;
    // <https://github.com/filecoin-project/builtin-actors/blob/8d957d2901c0f2044417c268f0511324f591cb92/runtime/src/runtime/policy.rs#L299>
    pub const AddressedSectorsMax: u64 = 25_000;
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
    type PeerId = BoundedVec<u8, ConstU32<PEER_ID_MAX_BYTES>>; // https://github.com/libp2p/specs/blob/master/peer-ids/peer-ids.md#peer-ids
    type Currency = Balances;
    type Market = Market;
    type ProofVerification = primitives::testing::DummyProofsVerification;
    type WPoStProvingPeriod = WPostProvingPeriod;
    type WPoStChallengeWindow = WPostChallengeWindow;
    type WPoStChallengeLookBack = WPostChallengeLookBack;
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

impl pallet_proofs::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
}

impl crate::pallet::Config for Test {}
