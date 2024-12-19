// This is free and unencumbered software released into the public domain.
//
// Anyone is free to copy, modify, publish, use, compile, sell, or
// distribute this software, either in source code form or as a compiled
// binary, for any purpose, commercial or non-commercial, and by any
// means.
//
// In jurisdictions that recognize copyright laws, the author or authors
// of this software dedicate any and all copyright interest in the
// software to the public domain. We make this dedication for the benefit
// of the public at large and to the detriment of our heirs and
// successors. We intend this dedication to be an overt act of
// relinquishment in perpetuity of all present and future rights to this
// software under copyright law.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
// OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
// OTHER DEALINGS IN THE SOFTWARE.
//
// For more information, please refer to <http://unlicense.org>

mod xcm_config;

// Substrate and Polkadot dependencies
use cumulus_pallet_parachain_system::RelayNumberMonotonicallyIncreases;
use cumulus_primitives_core::{AggregateMessageOrigin, ParaId};
use frame_support::{
    derive_impl,
    dispatch::DispatchClass,
    parameter_types,
    traits::{ConstBool, ConstU32, ConstU64, ConstU8, EitherOfDiverse, TransformOrigin},
    weights::{ConstantMultiplier, Weight},
    BoundedVec, PalletId,
};
use frame_system::{
    limits::{BlockLength, BlockWeights},
    EnsureRoot,
};
use pallet_xcm::{EnsureXcm, IsVoiceOfBody};
use parachains_common::message_queue::{NarrowOriginToSibling, ParaIdToSibling};
use polkadot_runtime_common::{
    xcm_sender::NoPriceForMessageDelivery, BlockHashCount, SlowAdjustingFeeUpdate,
};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_runtime::{traits::Verify, MultiSignature, Perbill};
use sp_version::RuntimeVersion;
use xcm::latest::prelude::BodyId;
use xcm_config::{RelayLocation, XcmOriginToTransactDispatchOrigin};

// Local module imports
use super::{
    weights::{BlockExecutionWeight, ExtrinsicBaseWeight, RocksDbWeight},
    AccountId, Aura, Balance, Balances, Block, BlockNumber, CollatorSelection, ConsensusHook, Hash,
    MessageQueue, Nonce, PalletInfo, ParachainSystem, Runtime, RuntimeCall, RuntimeEvent,
    RuntimeFreezeReason, RuntimeHoldReason, RuntimeOrigin, RuntimeTask, Session, SessionKeys,
    System, WeightToFee, XcmpQueue, AVERAGE_ON_INITIALIZE_RATIO, EXISTENTIAL_DEPOSIT, HOURS,
    MAXIMUM_BLOCK_WEIGHT, MICROUNIT, NORMAL_DISPATCH_RATIO, SLOT_DURATION, VERSION,
};
use crate::{BabeDataGetter, DAYS, MINUTES};

parameter_types! {
    pub const Version: RuntimeVersion = VERSION;

    // This part is copied from Substrate's `bin/node/runtime/src/lib.rs`.
    //  The `RuntimeBlockLength` and `RuntimeBlockWeights` exist here because the
    // `DeletionWeightLimit` and `DeletionQueueDepth` depend on those to parameterize
    // the lazy contract deletion.
    pub RuntimeBlockLength: BlockLength =
        BlockLength::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
    pub RuntimeBlockWeights: BlockWeights = BlockWeights::builder()
        .base_block(BlockExecutionWeight::get())
        .for_class(DispatchClass::all(), |weights| {
            weights.base_extrinsic = ExtrinsicBaseWeight::get();
        })
        .for_class(DispatchClass::Normal, |weights| {
            weights.max_total = Some(NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT);
        })
        .for_class(DispatchClass::Operational, |weights| {
            weights.max_total = Some(MAXIMUM_BLOCK_WEIGHT);
            // Operational transactions have some extra reserved space, so that they
            // are included even if block reached `MAXIMUM_BLOCK_WEIGHT`.
            weights.reserved = Some(
                MAXIMUM_BLOCK_WEIGHT - NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT
            );
        })
        .avg_block_initialization(AVERAGE_ON_INITIALIZE_RATIO)
        .build_or_panic();
    pub const SS58Prefix: u16 = 42;
}

/// The default types are being injected by [`derive_impl`](`frame_support::derive_impl`) from
/// [`ParaChainDefaultConfig`](`struct@frame_system::config_preludes::ParaChainDefaultConfig`),
/// but overridden as needed.
#[derive_impl(frame_system::config_preludes::ParaChainDefaultConfig)]
impl frame_system::Config for Runtime {
    /// The identifier used to distinguish between accounts.
    type AccountId = AccountId;
    /// The index type for storing how many extrinsics an account has signed.
    type Nonce = Nonce;
    /// The type for hashing blocks and tries.
    type Hash = Hash;
    /// The block type.
    type Block = Block;
    /// Maximum number of block number to block hash mappings to keep (oldest pruned first).
    type BlockHashCount = BlockHashCount;
    /// Runtime version.
    type Version = Version;
    /// The data to be stored in an account.
    type AccountData = pallet_balances::AccountData<Balance>;
    /// The weight of database operations that the runtime can invoke.
    type DbWeight = RocksDbWeight;
    /// Block & extrinsics weights: base values and limits.
    type BlockWeights = RuntimeBlockWeights;
    /// The maximum length of a block (in bytes).
    type BlockLength = RuntimeBlockLength;
    /// This is used as an identifier of the chain. 42 is the generic substrate prefix.
    type SS58Prefix = SS58Prefix;
    /// The action to take on a Runtime Upgrade
    type OnSetCode = cumulus_pallet_parachain_system::ParachainSetCode<Self>;
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

impl pallet_timestamp::Config for Runtime {
    /// A timestamp: milliseconds since the unix epoch.
    type Moment = u64;
    type OnTimestampSet = Aura;
    type MinimumPeriod = ConstU64<0>;
    type WeightInfo = ();
}

impl pallet_authorship::Config for Runtime {
    type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Aura>;
    type EventHandler = (CollatorSelection,);
}

parameter_types! {
    pub const ExistentialDeposit: Balance = EXISTENTIAL_DEPOSIT;
}

impl pallet_balances::Config for Runtime {
    type MaxLocks = ConstU32<50>;
    /// The type for recording an account's balance.
    type Balance = Balance;
    /// The ubiquitous event type.
    type RuntimeEvent = RuntimeEvent;
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
    type MaxReserves = ConstU32<50>;
    type ReserveIdentifier = [u8; 8];
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type FreezeIdentifier = ();
    type MaxFreezes = ConstU32<0>;
    type DoneSlashHandler = ();
}

parameter_types! {
    /// Relay Chain `TransactionByteFee` / 10
    pub const TransactionByteFee: Balance = 10 * MICROUNIT;
}

impl pallet_transaction_payment::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type OnChargeTransaction = pallet_transaction_payment::FungibleAdapter<Balances, ()>;
    type WeightToFee = WeightToFee;
    type LengthToFee = ConstantMultiplier<Balance, TransactionByteFee>;
    type FeeMultiplierUpdate = SlowAdjustingFeeUpdate<Self>;
    type OperationalFeeMultiplier = ConstU8<5>;
    type WeightInfo = ();
}

impl pallet_sudo::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type WeightInfo = ();
}

parameter_types! {
    pub const ReservedXcmpWeight: Weight = MAXIMUM_BLOCK_WEIGHT.saturating_div(4);
    pub const ReservedDmpWeight: Weight = MAXIMUM_BLOCK_WEIGHT.saturating_div(4);
    pub const RelayOrigin: AggregateMessageOrigin = AggregateMessageOrigin::Parent;
}

impl cumulus_pallet_parachain_system::Config for Runtime {
    type WeightInfo = ();
    type RuntimeEvent = RuntimeEvent;
    type OnSystemEvent = ();
    type SelfParaId = parachain_info::Pallet<Runtime>;
    type OutboundXcmpMessageSource = XcmpQueue;
    type DmpQueue = frame_support::traits::EnqueueWithOrigin<MessageQueue, RelayOrigin>;
    type ReservedDmpWeight = ReservedDmpWeight;
    type XcmpMessageHandler = XcmpQueue;
    type ReservedXcmpWeight = ReservedXcmpWeight;
    type CheckAssociatedRelayNumber = RelayNumberMonotonicallyIncreases;
    type ConsensusHook = ConsensusHook;
    type SelectCore = cumulus_pallet_parachain_system::DefaultCoreSelector<Runtime>;
}

impl parachain_info::Config for Runtime {}

parameter_types! {
    pub MessageQueueServiceWeight: Weight = Perbill::from_percent(35) * RuntimeBlockWeights::get().max_block;
}

impl pallet_message_queue::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
    #[cfg(feature = "runtime-benchmarks")]
    type MessageProcessor = pallet_message_queue::mock_helpers::NoopMessageProcessor<
        cumulus_primitives_core::AggregateMessageOrigin,
    >;
    #[cfg(not(feature = "runtime-benchmarks"))]
    type MessageProcessor = xcm_builder::ProcessXcmMessage<
        AggregateMessageOrigin,
        xcm_executor::XcmExecutor<xcm_config::XcmConfig>,
        RuntimeCall,
    >;
    type Size = u32;
    // The XCMP queue pallet is only ever able to handle the `Sibling(ParaId)` origin:
    type QueueChangeHandler = NarrowOriginToSibling<XcmpQueue>;
    type QueuePausedQuery = NarrowOriginToSibling<XcmpQueue>;
    type HeapSize = sp_core::ConstU32<{ 64 * 1024 }>;
    type MaxStale = sp_core::ConstU32<8>;
    type ServiceWeight = MessageQueueServiceWeight;
    type IdleMaxServiceWeight = ();
}

impl cumulus_pallet_aura_ext::Config for Runtime {}

impl cumulus_pallet_xcmp_queue::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type ChannelInfo = ParachainSystem;
    type VersionWrapper = ();
    // Enqueue XCMP messages from siblings for later processing.
    type XcmpQueue = TransformOrigin<MessageQueue, AggregateMessageOrigin, ParaId, ParaIdToSibling>;
    type MaxInboundSuspended = sp_core::ConstU32<1_000>;
    type MaxActiveOutboundChannels = sp_core::ConstU32<128>;
    type MaxPageSize = ConstU32<{ 1 << 16 }>;
    type ControllerOrigin = EnsureRoot<AccountId>;
    type ControllerOriginConverter = XcmOriginToTransactDispatchOrigin;
    type WeightInfo = ();
    type PriceForSiblingDelivery = NoPriceForMessageDelivery<ParaId>;
}

parameter_types! {
    pub const Period: u64 = 6 * HOURS;
    pub const Offset: u64 = 0;
}

impl pallet_session::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type ValidatorId = <Self as frame_system::Config>::AccountId;
    // we don't have stash and controller, thus we don't need the convert as well.
    type ValidatorIdOf = pallet_collator_selection::IdentityCollator;
    type ShouldEndSession = pallet_session::PeriodicSessions<Period, Offset>;
    type NextSessionRotation = pallet_session::PeriodicSessions<Period, Offset>;
    type SessionManager = CollatorSelection;
    // Essentially just Aura, but let's be pedantic.
    type SessionHandler = <SessionKeys as sp_runtime::traits::OpaqueKeys>::KeyTypeIdProviders;
    type Keys = SessionKeys;
    type WeightInfo = ();
}

impl pallet_aura::Config for Runtime {
    type AuthorityId = AuraId;
    type DisabledValidators = ();
    type MaxAuthorities = ConstU32<100_000>;
    type AllowMultipleBlocksPerSlot = ConstBool<true>;
    type SlotDuration = ConstU64<SLOT_DURATION>;
}

parameter_types! {
    pub const PotId: PalletId = PalletId(*b"PotStake");
    pub const SessionLength: BlockNumber = 6 * HOURS;
    // StakingAdmin pluralistic body.
    pub const StakingAdminBodyId: BodyId = BodyId::Defense;
}

/// We allow root and the StakingAdmin to execute privileged collator selection operations.
pub type CollatorSelectionUpdateOrigin = EitherOfDiverse<
    EnsureRoot<AccountId>,
    EnsureXcm<IsVoiceOfBody<RelayLocation, StakingAdminBodyId>>,
>;

impl pallet_collator_selection::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type UpdateOrigin = CollatorSelectionUpdateOrigin;
    type PotId = PotId;
    type MaxCandidates = ConstU32<100>;
    type MinEligibleCollators = ConstU32<4>;
    type MaxInvulnerables = ConstU32<20>;
    // should be a multiple of session or things will get inconsistent
    type KickThreshold = Period;
    type ValidatorId = <Self as frame_system::Config>::AccountId;
    type ValidatorIdOf = pallet_collator_selection::IdentityCollator;
    type ValidatorRegistration = Session;
    type WeightInfo = ();
}

#[cfg(not(feature = "testnet"))]
parameter_types! {
    // Storage Provider Pallet
    pub const WpostProvingPeriod: BlockNumber = DAYS;
    // Half an hour (=48 per day)
    // 30 * 60 = 30 minutes
    // SLOT_DURATION is in milliseconds thats why we / 1000
    pub const WpostChallengeWindow: BlockNumber = 30 * 60 / (SLOT_DURATION as BlockNumber / 1000);
    pub const WPoStChallengeLookBack: BlockNumber = 10 * MINUTES;
    pub const MinSectorExpiration: BlockNumber = 180 * DAYS;
    pub const MaxSectorExpiration: BlockNumber = 1278 * DAYS;
    pub const SectorMaximumLifetime: BlockNumber = (365 * DAYS) * 5; // 5 years
    pub const MaxProveCommitDuration: BlockNumber =  (30 * DAYS) + 150;
    pub const WPoStPeriodDeadlines: u64 = 48;
    pub const MaxPartitionsPerDeadline: u64 = 3000;
    pub const DeclarationsMax: u64 = 3000;
    pub const FaultMaxAge: BlockNumber = DAYS * 42;
    /// Copied from FileCoin and adapted to substrate block time. WPoStChallengeLookBack + 125 blocks
    /// <https://github.com/filecoin-project/builtin-actors/blob/6906288334746318385cfd53edd7ea33ef03919f/runtime/src/runtime/policy.rs#L327>
    pub const FaultDeclarationCutoff: BlockNumber = (10 * MINUTES) + 125;
    /// <https://github.com/filecoin-project/builtin-actors/blob/a45fb87910bca74d62215b0d58ed90cf78b6c8ff/runtime/src/runtime/policy.rs#L306>
    pub const PreCommitChallengeDelay: BlockNumber = 75 * MINUTES;
    // <https://github.com/filecoin-project/builtin-actors/blob/8d957d2901c0f2044417c268f0511324f591cb92/runtime/src/runtime/policy.rs#L299>
    pub const AddressedSectorsMax: u64 = 25_000;

    // Market Pallet
    /// Deal duration values copied from FileCoin.
    /// <https://github.com/filecoin-project/builtin-actors/blob/c32c97229931636e3097d92cf4c43ac36a7b4b47/actors/market/src/policy.rs#L28>
    pub const MinDealDuration: u64 = 20 * DAYS;
    pub const MaxDealDuration: u64 = 1278 * DAYS;

}

#[cfg(feature = "testnet")]
parameter_types! {
    // Storage Provider Pallet
    pub const WpostProvingPeriod: BlockNumber = 6 * MINUTES;
    pub const WPoStPeriodDeadlines: u64 = 3;
    pub const WpostChallengeWindow: BlockNumber = 2 * MINUTES;
    pub const WPoStChallengeLookBack: BlockNumber = MINUTES;
    pub const MinSectorExpiration: BlockNumber = 5 * MINUTES;
    pub const MaxSectorExpiration: BlockNumber = 60 * MINUTES;
    pub const SectorMaximumLifetime: BlockNumber = 120 * MINUTES;
    pub const MaxProveCommitDuration: BlockNumber = 5 * MINUTES;
    pub const MaxPartitionsPerDeadline: u64 = 3000;
    pub const FaultMaxAge: BlockNumber = (5 * MINUTES) * 42;
    pub const FaultDeclarationCutoff: BlockNumber = 1 * MINUTES;
    pub const PreCommitChallengeDelay: BlockNumber = 1 * MINUTES;
    // <https://github.com/filecoin-project/builtin-actors/blob/8d957d2901c0f2044417c268f0511324f591cb92/runtime/src/runtime/policy.rs#L299>
    pub const AddressedSectorsMax: u64 = 25_000;

    // Market Pallet
    pub const MinDealDuration: u64 = 5 * MINUTES;
    pub const MaxDealDuration: u64 = 180 * MINUTES;

    // Faucet pallet
    pub const FaucetDripAmount: Balance = 10_000_000_000_000;
    pub const FaucetDripDelay: BlockNumber = DAYS;
}

impl pallet_storage_provider::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Randomness = crate::Randomness;
    type AuthorVrfHistory = crate::Randomness;
    type PeerId = BoundedVec<u8, ConstU32<32>>; // Max length of SHA256 hash
    type Currency = Balances;
    type Market = crate::Market;
    type ProofVerification = crate::Proofs;
    type WPoStProvingPeriod = WpostProvingPeriod;
    type WPoStChallengeWindow = WpostChallengeWindow;
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

parameter_types! {
    /// PalletId of Market Pallet, used to convert it to AccountId which holds the Market funds
    pub const MarketPalletId: PalletId = PalletId(*b"spMarket");
}

pub type AccountPublic = <MultiSignature as Verify>::Signer;

impl pallet_market::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type PalletId = MarketPalletId;
    type OffchainSignature = MultiSignature;
    type OffchainPublic = AccountPublic;
    type StorageProviderValidation = crate::StorageProvider;
    type MaxDeals = ConstU32<128>;
    type MaxDealsPerBlock = ConstU32<128>;
    type MinDealDuration = MinDealDuration;
    type MaxDealDuration = MaxDealDuration;
}

impl pallet_proofs::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
}

#[cfg(feature = "testnet")]
impl pallet_faucet::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type FaucetDripAmount = FaucetDripAmount;
    type FaucetDripDelay = FaucetDripDelay;
}

/// Config for our randomness pallet
impl pallet_randomness::Config for Runtime {
    type AuthorVrfGetter = BabeDataGetter<Runtime>;
}
