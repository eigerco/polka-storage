extern crate alloc;

use frame_support::{
    derive_impl, pallet_prelude::ConstU32, parameter_types, sp_runtime::BoundedVec, PalletId,
};
use frame_system::pallet_prelude::BlockNumberFor;
use primitives::PEER_ID_MAX_BYTES;
use sp_arithmetic::traits::Zero;
use sp_runtime::{
    traits::{IdentifyAccount, IdentityLookup, Verify},
    MultiSignature,
};

type Block = frame_system::mocking::MockBlock<Test>;
type BlockNumber = BlockNumberFor<Test>;

const MILLISECS_PER_BLOCK: u64 = 6000;
const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber);

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

impl pallet_proofs::Config for Test {
    type Randomness = DummyRandomnessGenerator<Self>;
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
}

// Sourced from the Testnet runtime defined in <runtime/src/configs/mod.rs>.
parameter_types! {
    // Storage Provider Pallet
    pub const StoragePalletId: PalletId = PalletId(*b"Storage_");
    pub const WPoStProvingPeriod: BlockNumber = 6 * MINUTES;
    pub const WPoStPeriodDeadlines: u64 = 3;
    pub const WPoStChallengeWindow: BlockNumber = 2 * MINUTES;
    pub const WPoStChallengeLookBack: BlockNumber = MINUTES;
    pub const MinSectorExpiration: BlockNumber = 5 * MINUTES;
    pub const MaxSectorExpiration: BlockNumber = 60 * MINUTES;
    pub const SectorMaximumLifetime: BlockNumber = 120 * MINUTES;
    pub const MaxProveCommitDuration: BlockNumber = 5 * MINUTES;
    pub const MaxPartitionsPerDeadline: u64 = 3000;
    pub const FaultMaxAge: BlockNumber = (5 * MINUTES) * 42;
    pub const FaultDeclarationCutoff: BlockNumber = 1 * MINUTES;
    pub const PreCommitChallengeDelay: BlockNumber = 10;

    // <https://github.com/filecoin-project/builtin-actors/blob/8d957d2901c0f2044417c268f0511324f591cb92/runtime/src/runtime/policy.rs#L299>
    pub const AddressedSectorsMax: u64 = 25_000;

    // Market Pallet
    pub const MinDealDuration: BlockNumber = 5 * MINUTES;
    pub const MaxDealDuration: BlockNumber = 180 * MINUTES;
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
    type PalletId = StoragePalletId;
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
    type Currency = Balances;

    // Randomness Provider
    type Randomness = DummyRandomnessGenerator<Self>;
    type AuthorVrfHistory = DummyRandomnessGenerator<Self>;

    type PeerId = BoundedVec<u8, ConstU32<PEER_ID_MAX_BYTES>>; // https://github.com/libp2p/specs/blob/master/peer-ids/peer-ids.md#peer-ids

    // Proof Verification Provider
    type ProofVerification = Proofs;

    type OffchainSignature = Signature;
    type OffchainPublic = AccountPublic;
    type MaxDeals = ConstU32<32>;
    type MaxDealDuration = MaxDealDuration;
    type MaxDealsPerBlock = ConstU32<32>;
    type MinDealDuration = MinDealDuration;
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

impl crate::pallet::Config for Test {}
