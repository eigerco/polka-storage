extern crate alloc;

use frame_support::{
    derive_impl, pallet_prelude::ConstU32, parameter_types, sp_runtime::BoundedVec, PalletId,
};
use frame_system::pallet_prelude::BlockNumberFor;
use primitives::{configs::CurrencyProvider, PEER_ID_MAX_BYTES};
use sp_arithmetic::traits::Zero;
use sp_runtime::{
    traits::{IdentifyAccount, IdentityLookup, Verify},
    MultiSignature,
};

type Block = frame_system::mocking::MockBlock<Test>;
type BlockNumber = u64;

const MINUTES: BlockNumber = 1;

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

    #[runtime::pallet_index(4)]
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

impl CurrencyProvider for Test {
    type Currency = Balances;
}

impl pallet_market::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type PalletId = MarketPalletId;
    type WeightInfo = ();

    type StorageProviderValidation = StorageProvider;
    type MaxDealDuration = MaxDealDuration;
    type MaxDealsPerBlock = ConstU32<32>;
}

impl primitives::configs::MarketProvider for Test {
    type OffchainSignature = Signature;
    type OffchainPublic = AccountPublic;
    type MaxDeals = ConstU32<32>;
    type MinDealDuration = MinDealDuration;
}

impl pallet_proofs::Config for Test {
    type Randomness = DummyRandomnessGenerator<Self>;
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
}

// Sourced from the Testnet runtime defined in <runtime/src/configs/mod.rs>.
parameter_types! {
    // Storage Provider Pallet
    pub const WPoStPeriodDeadlines: u64 = 10;
    pub const WPoStProvingPeriod: BlockNumber = 40 * MINUTES;
    pub const WPoStChallengeWindow: BlockNumber = 4 * MINUTES;
    pub const WPoStChallengeLookBack: BlockNumber = MINUTES;
    pub const MinSectorExpiration: BlockNumber = 5 * MINUTES;
    pub const MaxSectorExpiration: BlockNumber = 360 * MINUTES;
    pub const SectorMaximumLifetime: BlockNumber = 120 * MINUTES;
    // NOTE: Changed from Testnet's 5 to 15 since the pre-commit delay is 10 blocks.
    pub const MaxProveCommitDuration: BlockNumber = 15 * MINUTES;
    pub const MaxPartitionsPerDeadline: u64 = 3000;
    pub const FaultMaxAge: BlockNumber = (5 * MINUTES) * 42;
    pub const FaultDeclarationCutoff: BlockNumber = 2 * MINUTES;
    // NOTE: Changed from Testnet's 0 to 10 to match the storage-provider client's `porep` command.
    pub const PreCommitChallengeDelay: BlockNumber = 10;
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

    // Randomness Provider
    type Randomness = DummyRandomnessGenerator<Self>;
    type AuthorVrfHistory = DummyRandomnessGenerator<Self>;

    type PeerId = BoundedVec<u8, ConstU32<PEER_ID_MAX_BYTES>>; // https://github.com/libp2p/specs/blob/master/peer-ids/peer-ids.md#peer-ids
    type Market = Market;

    // Proof Verification Provider
    type ProofVerification = Proofs;

    type WPoStProvingPeriod = WPoStProvingPeriod;
    type WPoStChallengeWindow = WPoStChallengeWindow;
    type WPoStChallengeLookBack = WPoStChallengeLookBack;
    type MinSectorExpiration = MinSectorExpiration;
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

impl primitives::configs::StorageProviderProvider for Test {
    type MaxSectorExpiration = MaxSectorExpiration;
    type SectorMaximumLifetime = SectorMaximumLifetime;
}

impl crate::pallet::Config for Test {}
