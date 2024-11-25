#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;
mod configs;
mod weights;

extern crate alloc;
use alloc::vec::Vec;

use frame_support::{
    genesis_builder_helper::{build_state, get_preset},
    weights::{
        constants::WEIGHT_REF_TIME_PER_SECOND, Weight, WeightToFeeCoefficient,
        WeightToFeeCoefficients, WeightToFeePolynomial,
    },
};
use pallet_aura::Authorities;
use smallvec::smallvec;
use sp_api::impl_runtime_apis;
pub use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{crypto::KeyTypeId, OpaqueMetadata};
#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;
use sp_runtime::{
    create_runtime_str, generic, impl_opaque_keys,
    traits::{BlakeTwo256, Block as BlockT, IdentifyAccount, Verify},
    transaction_validity::{TransactionSource, TransactionValidity},
    ApplyExtrinsicResult, MultiSignature,
};
pub use sp_runtime::{MultiAddress, Perbill, Permill};
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;
use weights::ExtrinsicBaseWeight;

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = MultiSignature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

/// Balance of an account.
pub type Balance = u128;

/// Index of a transaction in the chain.
pub type Nonce = u32;

/// A hash of some data used by the chain.
pub type Hash = sp_core::H256;

/// An index to a block.
pub type BlockNumber = u64;

/// The address format for describing accounts.
pub type Address = MultiAddress<AccountId, ()>;

/// Block header type as expected by this runtime.
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;

/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;

/// A Block signed with a Justification
pub type SignedBlock = generic::SignedBlock<Block>;

/// BlockId type as expected by this runtime.
pub type BlockId = generic::BlockId<Block>;

/// The SignedExtension to the basic transaction logic.
#[docify::export(template_signed_extra)]
pub type SignedExtra = (
    frame_system::CheckNonZeroSender<Runtime>,
    frame_system::CheckSpecVersion<Runtime>,
    frame_system::CheckTxVersion<Runtime>,
    frame_system::CheckGenesis<Runtime>,
    frame_system::CheckEra<Runtime>,
    frame_system::CheckNonce<Runtime>,
    frame_system::CheckWeight<Runtime>,
    pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
    cumulus_primitives_storage_weight_reclaim::StorageWeightReclaim<Runtime>,
);

/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic =
    generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;

/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
    Runtime,
    Block,
    frame_system::ChainContext<Runtime>,
    Runtime,
    AllPalletsWithSystem,
>;

/// Handles converting a weight scalar to a fee value, based on the scale and granularity of the
/// node's balance type.
///
/// This should typically create a mapping between the following ranges:
///   - `[0, MAXIMUM_BLOCK_WEIGHT]`
///   - `[Balance::min, Balance::max]`
///
/// Yet, it can be used for any other sort of change to weight-fee. Some examples being:
///   - Setting it to `0` will essentially disable the weight fee.
///   - Setting it to `1` will cause the literal `#[weight = x]` values to be charged.
pub struct WeightToFee;
impl WeightToFeePolynomial for WeightToFee {
    type Balance = Balance;
    fn polynomial() -> WeightToFeeCoefficients<Self::Balance> {
        // in Rococo, extrinsic base weight (smallest non-zero weight) is mapped to 1 MILLIUNIT:
        // in our template, we map to 1/10 of that, or 1/10 MILLIUNIT
        let p = MILLIUNIT / 10;
        let q = 100 * Balance::from(ExtrinsicBaseWeight::get().ref_time());
        smallvec![WeightToFeeCoefficient {
            degree: 1,
            negative: false,
            coeff_frac: Perbill::from_rational(p % q, q),
            coeff_integer: p / q,
        }]
    }
}

/// Opaque types. These are used by the CLI to instantiate machinery that don't need to know
/// the specifics of the runtime. They can then be made to be agnostic over specific formats
/// of data like extrinsics, allowing for them to continue syncing the network through upgrades
/// to even the core data structures.
pub mod opaque {
    pub use sp_runtime::OpaqueExtrinsic as UncheckedExtrinsic;
    use sp_runtime::{
        generic,
        traits::{BlakeTwo256, Hash as HashT},
    };

    use super::*;
    /// Opaque block header type.
    pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
    /// Opaque block type.
    pub type Block = generic::Block<Header, UncheckedExtrinsic>;
    /// Opaque block identifier type.
    pub type BlockId = generic::BlockId<Block>;
    /// Opaque block hash type.
    pub type Hash = <BlakeTwo256 as HashT>::Output;
}

impl_opaque_keys! {
    pub struct SessionKeys {
        pub aura: Aura,
    }
}

#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
    spec_name: create_runtime_str!("polka-storage-runtime"),
    impl_name: create_runtime_str!("polka-storage-runtime"),
    authoring_version: 1,
    spec_version: 1,
    impl_version: 0,
    apis: RUNTIME_API_VERSIONS,
    transaction_version: 1,
    state_version: 1,
};

#[docify::export]
mod block_times {
    /// This determines the average expected block time that we are targeting. Blocks will be
    /// produced at a minimum duration defined by `SLOT_DURATION`. `SLOT_DURATION` is picked up by
    /// `pallet_timestamp` which is in turn picked up by `pallet_aura` to implement `fn
    /// slot_duration()`.
    ///
    /// Change this to adjust the block time.
    pub const MILLISECS_PER_BLOCK: u64 = 6000;

    // NOTE: Currently it is not possible to change the slot duration after the chain has started.
    // Attempting to do so will brick block production.
    pub const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;
}
pub use block_times::*;

// Time is measured by number of blocks.
pub const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber);
pub const HOURS: BlockNumber = MINUTES * 60;
pub const DAYS: BlockNumber = HOURS * 24;

// Unit = the base number of indivisible units for balances
pub const UNIT: Balance = 1_000_000_000_000;
pub const MILLIUNIT: Balance = 1_000_000_000;
pub const MICROUNIT: Balance = 1_000_000;

/// The existential deposit. Set to 1/10 of the Connected Relay Chain.
pub const EXISTENTIAL_DEPOSIT: Balance = MILLIUNIT;

/// We assume that ~5% of the block weight is consumed by `on_initialize` handlers. This is
/// used to limit the maximal weight of a single extrinsic.
const AVERAGE_ON_INITIALIZE_RATIO: Perbill = Perbill::from_percent(5);

/// We allow `Normal` extrinsics to fill up the block up to 75%, the rest can be used by
/// `Operational` extrinsics.
const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

#[docify::export(max_block_weight)]
/// We allow for 2 seconds of compute with a 6 second average block time.
const MAXIMUM_BLOCK_WEIGHT: Weight = Weight::from_parts(
    WEIGHT_REF_TIME_PER_SECOND.saturating_mul(2),
    cumulus_primitives_core::relay_chain::MAX_POV_SIZE as u64,
);

#[docify::export]
mod async_backing_params {
    /// Maximum number of blocks simultaneously accepted by the Runtime, not yet included
    /// into the relay chain.
    pub(crate) const UNINCLUDED_SEGMENT_CAPACITY: u32 = 3;
    /// How many parachain blocks are processed by the relay chain per parent. Limits the
    /// number of blocks authored per slot.
    pub(crate) const BLOCK_PROCESSING_VELOCITY: u32 = 1;
    /// Relay chain slot duration, in milliseconds.
    pub(crate) const RELAY_CHAIN_SLOT_DURATION_MILLIS: u32 = 6000;
}
pub(crate) use async_backing_params::*;

#[docify::export]
/// Aura consensus hook
type ConsensusHook = cumulus_pallet_aura_ext::FixedVelocityConsensusHook<
    Runtime,
    RELAY_CHAIN_SLOT_DURATION_MILLIS,
    BLOCK_PROCESSING_VELOCITY,
    UNINCLUDED_SEGMENT_CAPACITY,
>;

/// The version information used to identify this runtime when compiled natively.
#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
    NativeVersion {
        runtime_version: VERSION,
        can_author_with: Default::default(),
    }
}

// Create the runtime by composing the FRAME pallets that were previously configured.
#[cfg(not(feature = "testnet"))]
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
    pub struct Runtime;

    #[runtime::pallet_index(0)]
    pub type System = frame_system;
    #[runtime::pallet_index(1)]
    pub type ParachainSystem = cumulus_pallet_parachain_system;
    #[runtime::pallet_index(2)]
    pub type Timestamp = pallet_timestamp;
    #[runtime::pallet_index(3)]
    pub type ParachainInfo = parachain_info;
    // Temporary. Will be removed after we switch to babe
    #[runtime::pallet_index(4)]
    pub type RandomnessSource = pallet_insecure_randomness_collective_flip;

    // Monetary stuff.
    #[runtime::pallet_index(10)]
    pub type Balances = pallet_balances;
    #[runtime::pallet_index(11)]
    pub type TransactionPayment = pallet_transaction_payment;

    // Governance
    #[runtime::pallet_index(15)]
    pub type Sudo = pallet_sudo;

    // Collator support. The order of these 4 are important and shall not change.
    #[runtime::pallet_index(20)]
    pub type Authorship = pallet_authorship;
    #[runtime::pallet_index(21)]
    pub type CollatorSelection = pallet_collator_selection;
    #[runtime::pallet_index(22)]
    pub type Session = pallet_session;
    #[runtime::pallet_index(23)]
    pub type Aura = pallet_aura;
    #[runtime::pallet_index(24)]
    pub type AuraExt = cumulus_pallet_aura_ext;

    // XCM helpers.
    #[runtime::pallet_index(30)]
    pub type XcmpQueue = cumulus_pallet_xcmp_queue;
    #[runtime::pallet_index(31)]
    pub type PolkadotXcm = pallet_xcm;
    #[runtime::pallet_index(32)]
    pub type CumulusXcm = cumulus_pallet_xcm;
    #[runtime::pallet_index(33)]
    pub type MessageQueue = pallet_message_queue;

    #[runtime::pallet_index(34)]
    pub type StorageProvider = pallet_storage_provider::pallet;
    #[runtime::pallet_index(35)]
    pub type Market = pallet_market;
    #[runtime::pallet_index(36)]
    pub type Proofs = pallet_proofs;
    #[runtime::pallet_index(37)]
    pub type Randomness = pallet_randomness;
}

// Create the runtime by composing the FRAME pallets that were previously configured.
#[cfg(feature = "testnet")]
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
    pub struct Runtime;

    #[runtime::pallet_index(0)]
    pub type System = frame_system;
    #[runtime::pallet_index(1)]
    pub type ParachainSystem = cumulus_pallet_parachain_system;
    #[runtime::pallet_index(2)]
    pub type Timestamp = pallet_timestamp;
    #[runtime::pallet_index(3)]
    pub type ParachainInfo = parachain_info;
    // Temporary. Will be removed after we switch to babe
    #[runtime::pallet_index(4)]
    pub type RandomnessSource = pallet_insecure_randomness_collective_flip;

    // Monetary stuff.
    #[runtime::pallet_index(10)]
    pub type Balances = pallet_balances;
    #[runtime::pallet_index(11)]
    pub type TransactionPayment = pallet_transaction_payment;

    // Governance
    #[runtime::pallet_index(15)]
    pub type Sudo = pallet_sudo;

    // Collator support. The order of these 4 are important and shall not change.
    #[runtime::pallet_index(20)]
    pub type Authorship = pallet_authorship;
    #[runtime::pallet_index(21)]
    pub type CollatorSelection = pallet_collator_selection;
    #[runtime::pallet_index(22)]
    pub type Session = pallet_session;
    #[runtime::pallet_index(23)]
    pub type Aura = pallet_aura;
    #[runtime::pallet_index(24)]
    pub type AuraExt = cumulus_pallet_aura_ext;

    // XCM helpers.
    #[runtime::pallet_index(30)]
    pub type XcmpQueue = cumulus_pallet_xcmp_queue;
    #[runtime::pallet_index(31)]
    pub type PolkadotXcm = pallet_xcm;
    #[runtime::pallet_index(32)]
    pub type CumulusXcm = cumulus_pallet_xcm;
    #[runtime::pallet_index(33)]
    pub type MessageQueue = pallet_message_queue;

    #[runtime::pallet_index(34)]
    pub type StorageProvider = pallet_storage_provider::pallet;
    #[runtime::pallet_index(35)]
    pub type Market = pallet_market;
    #[runtime::pallet_index(36)]
    pub type Proofs = pallet_proofs;
    #[runtime::pallet_index(37)]
    pub type Randomness = pallet_randomness;
    #[runtime::pallet_index(38)]
    pub type Faucet = pallet_faucet;
}

#[docify::export(register_validate_block)]
cumulus_pallet_parachain_system::register_validate_block! {
    Runtime = Runtime,
    BlockExecutor = cumulus_pallet_aura_ext::BlockExecutor::<Runtime, Executive>,
}

// we move some impls outside so we can easily use them with `docify`.
impl Runtime {
    #[docify::export]
    fn impl_slot_duration() -> sp_consensus_aura::SlotDuration {
        sp_consensus_aura::SlotDuration::from_millis(SLOT_DURATION)
    }

    #[docify::export]
    fn impl_can_build_upon(
        included_hash: <Block as BlockT>::Hash,
        slot: cumulus_primitives_aura::Slot,
    ) -> bool {
        ConsensusHook::can_build_upon(included_hash, slot)
    }
}

impl_runtime_apis! {
    impl primitives_proofs::StorageProviderApi<Block, AccountId> for Runtime {
        fn current_deadline(storage_provider: AccountId) -> u64 {
            0
        }
    }

    impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
        fn slot_duration() -> sp_consensus_aura::SlotDuration {
            Runtime::impl_slot_duration()
        }

        fn authorities() -> Vec<AuraId> {
            Authorities::<Runtime>::get().into_inner()
        }
    }

    impl cumulus_primitives_aura::AuraUnincludedSegmentApi<Block> for Runtime {
        fn can_build_upon(
            included_hash: <Block as BlockT>::Hash,
            slot: cumulus_primitives_aura::Slot,
        ) -> bool {
            Runtime::impl_can_build_upon(included_hash, slot)
        }
    }

    impl sp_api::Core<Block> for Runtime {
        fn version() -> RuntimeVersion {
            VERSION
        }

        fn execute_block(block: Block) {
            Executive::execute_block(block)
        }

        fn initialize_block(header: &<Block as BlockT>::Header) -> sp_runtime::ExtrinsicInclusionMode {
            Executive::initialize_block(header)
        }
    }

    impl sp_api::Metadata<Block> for Runtime {
        fn metadata() -> OpaqueMetadata {
            OpaqueMetadata::new(Runtime::metadata().into())
        }

        fn metadata_at_version(version: u32) -> Option<OpaqueMetadata> {
            Runtime::metadata_at_version(version)
        }

        fn metadata_versions() -> Vec<u32> {
            Runtime::metadata_versions()
        }
    }

    impl sp_block_builder::BlockBuilder<Block> for Runtime {
        fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
            Executive::apply_extrinsic(extrinsic)
        }

        fn finalize_block() -> <Block as BlockT>::Header {
            Executive::finalize_block()
        }

        fn inherent_extrinsics(data: sp_inherents::InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
            data.create_extrinsics()
        }

        fn check_inherents(
            block: Block,
            data: sp_inherents::InherentData,
        ) -> sp_inherents::CheckInherentsResult {
            data.check_extrinsics(&block)
        }
    }

    impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
        fn validate_transaction(
            source: TransactionSource,
            tx: <Block as BlockT>::Extrinsic,
            block_hash: <Block as BlockT>::Hash,
        ) -> TransactionValidity {
            Executive::validate_transaction(source, tx, block_hash)
        }
    }

    impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
        fn offchain_worker(header: &<Block as BlockT>::Header) {
            Executive::offchain_worker(header)
        }
    }

    impl sp_session::SessionKeys<Block> for Runtime {
        fn generate_session_keys(seed: Option<Vec<u8>>) -> Vec<u8> {
            SessionKeys::generate(seed)
        }

        fn decode_session_keys(
            encoded: Vec<u8>,
        ) -> Option<Vec<(Vec<u8>, KeyTypeId)>> {
            SessionKeys::decode_into_raw_public_keys(&encoded)
        }
    }

    impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Nonce> for Runtime {
        fn account_nonce(account: AccountId) -> Nonce {
            System::account_nonce(account)
        }
    }

    impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<Block, Balance> for Runtime {
        fn query_info(
            uxt: <Block as BlockT>::Extrinsic,
            len: u32,
        ) -> pallet_transaction_payment_rpc_runtime_api::RuntimeDispatchInfo<Balance> {
            TransactionPayment::query_info(uxt, len)
        }
        fn query_fee_details(
            uxt: <Block as BlockT>::Extrinsic,
            len: u32,
        ) -> pallet_transaction_payment::FeeDetails<Balance> {
            TransactionPayment::query_fee_details(uxt, len)
        }
        fn query_weight_to_fee(weight: Weight) -> Balance {
            TransactionPayment::weight_to_fee(weight)
        }
        fn query_length_to_fee(length: u32) -> Balance {
            TransactionPayment::length_to_fee(length)
        }
    }

    impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentCallApi<Block, Balance, RuntimeCall>
        for Runtime
    {
        fn query_call_info(
            call: RuntimeCall,
            len: u32,
        ) -> pallet_transaction_payment::RuntimeDispatchInfo<Balance> {
            TransactionPayment::query_call_info(call, len)
        }
        fn query_call_fee_details(
            call: RuntimeCall,
            len: u32,
        ) -> pallet_transaction_payment::FeeDetails<Balance> {
            TransactionPayment::query_call_fee_details(call, len)
        }
        fn query_weight_to_fee(weight: Weight) -> Balance {
            TransactionPayment::weight_to_fee(weight)
        }
        fn query_length_to_fee(length: u32) -> Balance {
            TransactionPayment::length_to_fee(length)
        }
    }

    impl cumulus_primitives_core::CollectCollationInfo<Block> for Runtime {
        fn collect_collation_info(header: &<Block as BlockT>::Header) -> cumulus_primitives_core::CollationInfo {
            ParachainSystem::collect_collation_info(header)
        }
    }

    #[cfg(feature = "try-runtime")]
    impl frame_try_runtime::TryRuntime<Block> for Runtime {
        fn on_runtime_upgrade(checks: frame_try_runtime::UpgradeCheckSelect) -> (Weight, Weight) {
            use super::configs::RuntimeBlockWeights;

            let weight = Executive::try_runtime_upgrade(checks).unwrap();
            (weight, RuntimeBlockWeights::get().max_block)
        }

        fn execute_block(
            block: Block,
            state_root_check: bool,
            signature_check: bool,
            select: frame_try_runtime::TryStateSelect,
        ) -> Weight {
            // NOTE: intentional unwrap: we don't want to propagate the error backwards, and want to
            // have a backtrace here.
            Executive::try_execute_block(block, state_root_check, signature_check, select).unwrap()
        }
    }

    #[cfg(feature = "runtime-benchmarks")]
    impl frame_benchmarking::Benchmark<Block> for Runtime {
        fn benchmark_metadata(extra: bool) -> (
            Vec<frame_benchmarking::BenchmarkList>,
            Vec<frame_support::traits::StorageInfo>,
        ) {
            use frame_benchmarking::{Benchmarking, BenchmarkList};
            use frame_support::traits::StorageInfoTrait;
            use frame_system_benchmarking::Pallet as SystemBench;
            use cumulus_pallet_session_benchmarking::Pallet as SessionBench;
            use super::*;

            let mut list = Vec::<BenchmarkList>::new();
            list_benchmarks!(list, extra);

            let storage_info = AllPalletsWithSystem::storage_info();
            (list, storage_info)
        }

        fn dispatch_benchmark(
            config: frame_benchmarking::BenchmarkConfig
        ) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, sp_runtime::RuntimeString> {
            use frame_benchmarking::{BenchmarkError, Benchmarking, BenchmarkBatch};
            use super::*;

            use frame_system_benchmarking::Pallet as SystemBench;
            impl frame_system_benchmarking::Config for Runtime {
                fn setup_set_code_requirements(code: &Vec<u8>) -> Result<(), BenchmarkError> {
                    ParachainSystem::initialize_for_set_code_benchmark(code.len() as u32);
                    Ok(())
                }

                fn verify_set_code() {
                    System::assert_last_event(cumulus_pallet_parachain_system::Event::<Runtime>::ValidationFunctionStored.into());
                }
            }

            use cumulus_pallet_session_benchmarking::Pallet as SessionBench;
            impl cumulus_pallet_session_benchmarking::Config for Runtime {}

            use frame_support::traits::WhitelistedStorageKeys;
            let whitelist = AllPalletsWithSystem::whitelisted_storage_keys();

            let mut batches = Vec::<BenchmarkBatch>::new();
            let params = (&config, &whitelist);
            add_benchmarks!(params, batches);

            if batches.is_empty() { return Err("Benchmark not found for this pallet.".into()) }
            Ok(batches)
        }
    }

    impl sp_genesis_builder::GenesisBuilder<Block> for Runtime {
        fn build_state(config: Vec<u8>) -> sp_genesis_builder::Result {
            build_state::<RuntimeGenesisConfig>(config)
        }

        fn get_preset(id: &Option<sp_genesis_builder::PresetId>) -> Option<Vec<u8>> {
            get_preset::<RuntimeGenesisConfig>(id, |_| None)
        }

        fn preset_names() -> Vec<sp_genesis_builder::PresetId> {
            Default::default()
        }
    }
}
