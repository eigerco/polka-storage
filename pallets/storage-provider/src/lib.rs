//! # Storage Provider Pallet
//!
//! This pallet is responsible for:
//! - Storage proving operations
//! - Used by the storage provider to generate and submit Proof-of-Replication (PoRep) and Proof-of-Spacetime (PoSt).
//! - Managing and handling collateral for storage deals, penalties, and rewards related to storage deal performance.
//!
//! This pallet holds information about storage providers and provides an interface to modify that information.
//!
//! The Storage Provider Pallet is the source of truth for anything storage provider related.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod tests;

mod deadline;
pub mod deal;
mod dispatchables;
pub mod error;
pub mod expiration_queue;
pub mod fault;
mod hooks;
mod partition;
pub mod proofs;
pub mod sector;
mod sector_map;
mod storage_provider;
pub mod weights;

pub use pallet::*;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    pub(crate) const DECLARATIONS_MAX: u32 = 3000;
    pub(crate) const LOG_TARGET: &'static str = "runtime::storage_provider";

    extern crate alloc;

    use alloc::vec;
    use core::fmt::Debug;

    use codec::{Decode, Encode};
    use frame_support::{
        dispatch::DispatchResult,
        pallet_prelude::*,
        sp_runtime::traits::Hash,
        traits::{
            fungible::{Inspect, Mutate, MutateHold},
            Randomness,
        },
        PalletId,
    };
    use frame_system::pallet_prelude::{BlockNumberFor, *};
    use primitives::{
        deals::{ClientDealProposal, DealProposal},
        pallets::{DeadlineInfo as ExternalDeadlineInfo, ProofVerification},
        proofs::RegisteredPoStProof,
        randomness::AuthorVrfHistory,
        sector::{ProveCommitSector, SectorNumber, SectorPreCommitInfo},
        DealId, PartitionNumber, MAX_DEALS_PER_SECTOR, MAX_PARTITIONS_PER_DEADLINE, MAX_SECTORS,
        MAX_SECTORS_PER_CALL,
    };
    use scale_info::TypeInfo;
    use sp_runtime::traits::{AccountIdConversion, IdentifyAccount, Verify};

    use crate::{
        deadline::DeadlineInfo,
        deal::{
            parameters::{DealParameters, OffchainDealParameters},
            DealSettlementError, PublishedDeal, SettledDealData,
        },
        fault::{
            DeclareFaultsParams, DeclareFaultsRecoveredParams, FaultDeclaration,
            RecoveryDeclaration,
        },
        proofs::SubmitWindowedPoStParams,
        sector::{ProveCommitResult, TerminateSectorsParams, TerminationDeclaration},
        storage_provider::{StorageProviderInfo, StorageProviderState},
        weights::WeightInfo,
    };

    /// Allows to extract Balance of an account via the Config::Currency associated type.
    /// BalanceOf is a sophisticated way of getting an u128.
    pub type BalanceOf<T> =
        <<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

    pub type DealProposalOf<T> =
        DealProposal<<T as frame_system::Config>::AccountId, BalanceOf<T>, BlockNumberFor<T>>;

    #[pallet::pallet]
    #[pallet::without_storage_info] // Allows to define storage items without fixed size
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Overarching hold reason.
        type RuntimeHoldReason: From<HoldReason>;

        /// The currency mechanism.
        type Currency: MutateHold<Self::AccountId, Reason = Self::RuntimeHoldReason>
            + Mutate<Self::AccountId>;

        /// The pallet weights;
        type WeightInfo: WeightInfo;

        /// Randomness generator
        type Randomness: Randomness<Self::Hash, BlockNumberFor<Self>>;

        /// PalletId used to derive AccountId which stores funds of the Market Participants.
        #[pallet::constant]
        type PalletId: Get<PalletId>;

        /// The Multiaddr type
        type Multiaddr: Clone + Debug + Decode + Encode + Eq + TypeInfo;

        /// Proof verification trait implementation for verifying proofs.
        type ProofVerification: ProofVerification;

        /// Trait for AuthorVRF querying.
        type AuthorVrfHistory: AuthorVrfHistory<BlockNumberFor<Self>, Self::Hash>;

        /// Off-Chain signature type.
        ///
        /// Can verify whether an `Self::OffchainPublic` created a signature.
        type OffchainSignature: Verify<Signer = Self::OffchainPublic> + Parameter;

        /// Off-Chain public key.
        ///
        /// Must identify as an on-chain `Self::AccountId`.
        type OffchainPublic: IdentifyAccount<AccountId = <Self as frame_system::Config>::AccountId>;

        /// How many deals can be published in a single batch of `publish_storage_deals`.
        // TODO(@Jinxit,17/04/2025): Turn this into a function same like `min_deal_duration` and re-add
        //                           the #[pallet::constant] in the pallet.
        type MaxDeals: Get<u32>;

        /// How many days should a deal last (activated). Maximum.
        /// Filecoin uses 1278 as default.
        /// https://github.com/filecoin-project/builtin-actors/blob/c32c97229931636e3097d92cf4c43ac36a7b4b47/actors/market/src/policy.rs#L29
        #[pallet::constant]
        type MaxDealDuration: Get<BlockNumberFor<Self>>;

        /// How many deals can be scheduled to start at a given block. Maximum.
        /// Those deals are checked by Hook::<T>::on_initialize and it has to have reasonable time complexity.
        /// Having this number too big can affect block production.
        #[pallet::constant]
        type MaxDealsPerBlock: Get<u32>;

        /// How many days should a deal last (activated). Minimum.
        /// Filecoin uses 180 as default.
        /// https://github.com/filecoin-project/builtin-actors/blob/c32c97229931636e3097d92cf4c43ac36a7b4b47/actors/market/src/policy.rs#L29
        #[pallet::constant]
        type MinDealDuration: Get<BlockNumberFor<Self>>;

        /// Window PoSt proving period — equivalent to 24 hours worth of blocks.
        ///
        /// During the proving period, storage providers submit Spacetime proofs over smaller
        /// intervals that make it unreasonable to cheat the system, if they fail to provide a proof
        /// in time, they will get slashed.
        ///
        /// In Filecoin, this concept starts with wall time — i.e. 24 hours — and is quantized into
        /// discrete blocks. In our case, we need to consistently put out blocks, every 12 seconds
        /// or 5 blocks per minute, as such, we instead work by block numbers only.
        ///
        /// For example, consider that the first proving period was started at block `0`, to figure
        /// out the proving period for an arbitrary block we must perform integer division between
        /// the block number and the amount of blocks expected to be produced in 24 hours:
        ///
        /// ```text
        /// proving_period = current_block // DAYS
        /// ```
        ///
        /// If we produce 5 blocks per minute, in an hour, we produce `60 * 5 = 300`, following that
        /// we produce `24 * 300 = 7200` blocks per day.
        ///
        /// Hence, if we're in the block number `6873` we get `6873 // 7200 = 0` meaning we are in
        /// the proving period `0`; moving that forward, consider the block `745711`, we'll get
        /// `745711 // 7200 = 103`, thus, we're in the proving period `103`.
        ///
        /// References:
        /// * <https://spec.filecoin.io/#section-algorithms.pos.post.design>
        /// * <https://spec.filecoin.io/#section-systems.filecoin_mining.storage_mining.proof-of-spacetime>
        #[pallet::constant]
        type WPoStProvingPeriod: Get<BlockNumberFor<Self>>;

        /// Window PoSt challenge window — equivalent to 30 minutes worth of blocks.
        ///
        /// To better understand the following explanation, read [`WPoStProvingPeriod`] first.
        ///
        /// During the Window PoSt proving period, challenges are issued to storage providers to
        /// prove they are still (correctly) storing the data they accepted, in the case of failure
        /// the storage provider will get slashed and have the sector marked as faulty.
        ///
        /// Given that our system works around block numbers, we have time quantization by default,
        /// however it still is necessary to figure out where we stand in the current challenge
        /// window.
        ///
        /// Since we know that, in Filecoin, each 24 hour period is subdivided into 30 minute
        /// epochs, we also subdivide our 24 hour period by 48, just in blocks.
        ///
        /// Consider the block number `745711` (like in the [`WPoStProvingPeriod`]) and that every
        /// 30 minutes, we produce `150` blocks (`300 blocks / hour // 2`). To calculate the current
        /// challenge window we perform the following steps:
        ///
        /// 1. calculate the current proving period — `745711 // 7200 = 103`
        /// 2. calculate the start of said proving period — `103 * 7200 = 741600`
        /// 3. calculate how many blocks elapsed since the beginning of said proving period —
        ///    `745711 - 741600 = 4111`
        /// 4. calculate the number of elapsed challenge windows — `4111 // 150 = 27`
        ///
        /// In some cases, it will be helpful to calculate the next deadline as well, picking up
        /// where we left, we perform the following steps:
        ///
        /// 5. calculate the block in which the current challenge window started —
        ///    for the "sub-block" `27 * 150 = 4050` & for the block `103 * 7200 + 4050 = 745650`
        /// 6. calculate the next deadline — `745650 + 150 = 745800`
        ///
        /// References:
        /// * <https://spec.filecoin.io/#section-algorithms.pos.post.design>
        #[pallet::constant]
        type WPoStChallengeWindow: Get<BlockNumberFor<Self>>;

        /// Window PoSt challenge look back. This lookback exists so that
        /// deadline windows can be non-overlapping (which makes the programming
        /// simpler). This period allows the storage providers to start working
        /// on the post before the deadline is officially opened to receiving a
        /// PoSt.
        #[pallet::constant]
        type WPoStChallengeLookBack: Get<BlockNumberFor<Self>>;

        /// Minimum number of blocks past the current block a sector may be set to expire.
        #[pallet::constant]
        type MinSectorExpiration: Get<BlockNumberFor<Self>>;

        /// Maximum number of blocks past the current block a sector may be set to expire.
        #[pallet::constant]
        type MaxSectorExpiration: Get<BlockNumberFor<Self>>;

        /// Maximum duration to allow for the sealing process for seal algorithms.
        #[pallet::constant]
        type MaxProveCommitDuration: Get<BlockNumberFor<Self>>;

        /// Maximum number of blocks a sector can stay in pre-committed state
        #[pallet::constant]
        type SectorMaximumLifetime: Get<BlockNumberFor<Self>>;

        /// Represents how many challenge deadline there are in 1 proving period.
        /// Closely tied to `WPoStChallengeWindow`
        /// It needs to be at least 3, because if we prove commit at a deadline,
        /// the system cannot assign a sector directly to a next one.
        /// https://github.com/eigerco/polka-storage/blob/8c01c3cf65e5caee7a191df367dda4a66e27594b/pallets/storage-provider/src/deadline.rs#L784
        #[pallet::constant]
        type WPoStPeriodDeadlines: Get<u64>;

        #[pallet::constant]
        type MaxPartitionsPerDeadline: Get<u64>;

        /// The longest a faulty sector can live without being removed.
        #[pallet::constant]
        type FaultMaxAge: Get<BlockNumberFor<Self>>;

        /// The period before a PoSt window closes its fault declaration and recovery
        /// <https://github.com/filecoin-project/builtin-actors/blob/82d02e58f9ef456aeaf2a6c737562ac97b22b244/runtime/src/runtime/policy.rs#L327-L328>
        #[pallet::constant]
        type FaultDeclarationCutoff: Get<BlockNumberFor<Self>>;

        /// Number of blocks between publishing the precommit and when the
        /// challenge for interactive PoRep can be first drawn from the randomness.
        #[pallet::constant]
        type PreCommitChallengeDelay: Get<BlockNumberFor<Self>>;

        /// The maximum number of partitions that may be required to be loaded in a single invocation.
        /// This limits the number of simultaneous fault, recovery, or sector-extension declarations.
        type AddressedPartitionsMax: Get<u64>;

        /// The maximum number of sector numbers addressable in a single invocation
        /// (which implies also the max infos that may be loaded at once).
        type AddressedSectorsMax: Get<u64>;
    }

    /// Need some storage type that keeps track of sectors, deadlines and terminations.
    #[pallet::storage]
    #[pallet::getter(fn storage_providers)]
    pub type StorageProviders<T: Config> = StorageMap<
        _,
        _,
        T::AccountId,
        StorageProviderState<T::Multiaddr, BalanceOf<T>, BlockNumberFor<T>>,
    >;

    /// Simple incremental ID generator for `Deal` Identification purposes.
    /// Starts as 0, increments once for each published deal.
    /// [`DealId`] is monotonically incremented, does not wrap around.
    /// If there is more [`DealId`]s then u64, panics the runtime (if the chain processed 1M deals / day, it would take ~50539024859 years
    /// to reach the ID limit — for reference Filecoin doesn't even average 200k / day).
    #[pallet::storage]
    pub type NextDealId<T: Config> = StorageValue<_, DealId, ValueQuery>;

    /// Stores all published proposals which are handled by the Market.
    /// Deals are identified by `DealId`.
    /// Proposals are stored here until terminated and settled or expired (not activated in time).
    #[pallet::storage]
    pub type Proposals<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        DealId,
        DealProposal<T::AccountId, BalanceOf<T>, BlockNumberFor<T>>,
    >;

    /// Stores Proposals which have been Published but not yet Activated.
    /// Only `T::MaxDeals` Pending Proposals can be held at any time.
    /// `hash_proposal(deal)` is stored in the [`BoundedBTreeSet`].
    /// Stores the Pending Proposals to deduplicate Deals and don't allow to same deal to be Published twice.
    /// Deals could end up having different DealId, but same contents. New deals cannot be deduplicated based on DealId.
    #[pallet::storage]
    pub type PendingProposals<T: Config> =
        StorageValue<_, BoundedBTreeSet<T::Hash, T::MaxDeals>, ValueQuery>;

    /// Stores Published or Activated Deals for each Block.
    /// When Deal is Published it's expected to be activated until a certain Block.
    /// If it's not, Storage Provider is slashed and Client refunded by [`Hooks::on_finalize`].
    /// If it has been activated properly, it's just removed from the map.
    #[pallet::storage]
    pub type DealsForBlock<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        BlockNumberFor<T>,
        BoundedBTreeSet<DealId, T::MaxDealsPerBlock>,
        ValueQuery,
    >;

    /// Holds a mapping from ([`Provider`] [`SectorNumber`]) to its respective [`DealId`]s.
    #[pallet::storage]
    pub type SectorDeals<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        (T::AccountId, SectorNumber),
        BoundedVec<DealId, ConstU32<MAX_DEALS_PER_SECTOR>>,
    >;

    /// Holds deal parameters for storage provider
    #[pallet::storage]
    pub type SPDealParameters<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        DealParameters<BalanceOf<T>, BlockNumberFor<T>>,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Market Participant deposited free balance to the Market Account.
        BalanceAdded {
            who: T::AccountId,
            amount: BalanceOf<T>,
        },
        /// Market Participant withdrawn their free balance from the Market Account.
        BalanceWithdrawn {
            who: T::AccountId,
            amount: BalanceOf<T>,
        },
        /// Deal has been successfully activated.
        DealActivated {
            deal_id: DealId,
            client: T::AccountId,
            provider: T::AccountId,
        },
        /// Deals were settled.
        DealsSettled {
            /// Deal IDs for those that were successfully settled.
            successful: BoundedVec<SettledDealData<T>, T::MaxDeals>,
            /// Deal IDs for those that were not successfully settled along with the respective error.
            unsuccessful: BoundedVec<(DealId, DealSettlementError), T::MaxDeals>,
        },
        /// Deal was slashed.
        /// It means that the `provider_collateral` was burned and the entire client's lockup returned.
        ///
        /// Currently it's emitted only when a deal was supposed to be activated on a given block, but was not.
        /// [`Hooks::on_finalize`] checks deals and slashes them when necessary.
        DealSlashed {
            deal_id: DealId,
            amount: BalanceOf<T>,
            client: T::AccountId,
            provider: T::AccountId,
        },
        /// Deal has been terminated.
        ///
        /// A deal may be voluntarily terminated by the storage provider,
        /// or involuntarily, if the sector has been faulty for 42 consecutive days.
        ///
        /// Source: <https://spec.filecoin.io/#section-systems.filecoin_mining.sector.lifecycle>
        DealTerminated {
            deal_id: DealId,
            client: T::AccountId,
            provider: T::AccountId,
        },
        /// Batch of published deals.
        DealsPublished {
            provider: T::AccountId,
            deals: BoundedVec<PublishedDeal<T>, T::MaxDeals>,
        },
        /// An SP has updated or published their deal parameters
        DealParametersUpdated {
            provider: T::AccountId,
            deal_parameters: DealParameters<BalanceOf<T>, BlockNumberFor<T>>,
        },
        /// An SP has removed their deal parameters
        DealParametersRemoved { provider: T::AccountId },
        /// Emitted when a new storage provider is registered.
        StorageProviderRegistered {
            owner: T::AccountId,
            info: StorageProviderInfo<T::Multiaddr>,
            proving_period_start: BlockNumberFor<T>,
        },
        /// Emitted when a storage provider deregisters.
        StorageProviderDeregistered {
            owner: T::AccountId,
            info: StorageProviderInfo<T::Multiaddr>,
        },
        /// Emitted when a storage provider pre commits some sectors.
        SectorsPreCommitted {
            /// Block at which sectors have been precommitted.
            /// It is used for `interactive_randomness` generation in `ProveCommit`.
            block: BlockNumberFor<T>,
            owner: T::AccountId,
            sectors:
                BoundedVec<SectorPreCommitInfo<BlockNumberFor<T>>, ConstU32<MAX_SECTORS_PER_CALL>>,
        },
        /// Emitted when a storage provider successfully proves pre committed sectors.
        SectorsProven {
            owner: T::AccountId,
            sectors: BoundedVec<ProveCommitResult, ConstU32<MAX_SECTORS_PER_CALL>>,
        },
        /// Emitted when a sector was pre-committed, but not proven, so it got slashed in the pre-commit hook.
        SectorsSlashed {
            owner: T::AccountId,
            // No need for a bounded collection as we produce the output ourselves.
            sector_numbers: BoundedVec<SectorNumber, ConstU32<MAX_SECTORS>>,
        },
        /// Emitted when an SP submits a valid PoSt
        ValidPoStSubmitted { owner: T::AccountId },
        /// Emitted when an SP declares some sectors as faulty
        FaultsDeclared {
            owner: T::AccountId,
            faults: BoundedVec<FaultDeclaration, ConstU32<DECLARATIONS_MAX>>,
        },
        /// Emitted when an SP declares some sectors as recovered
        FaultsRecovered {
            owner: T::AccountId,
            recoveries: BoundedVec<RecoveryDeclaration, ConstU32<DECLARATIONS_MAX>>,
        },
        /// Emitted when an SP doesn't submit Windowed PoSt in time and PoSt hook marks partitions as faulty
        PartitionsFaulty {
            owner: T::AccountId,
            faulty_partitions: BoundedBTreeMap<
                PartitionNumber,
                BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,
                ConstU32<MAX_PARTITIONS_PER_DEADLINE>,
            >,
        },
        /// Emitted when an SP terminates some sectors.
        SectorsTerminated {
            owner: T::AccountId,
            terminations: BoundedVec<TerminationDeclaration, ConstU32<DECLARATIONS_MAX>>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// Market Participant tries to withdraw more
        /// funds than they have available on the Market, because:
        /// - they never deposited the amount they want to withdraw
        /// - the funds they deposited were locked as part of a deal
        InsufficientFreeFunds,
        /// Tries to unlock funds which have never been locked before.
        InsufficientLockedFunds,
        /// `publish_storage_deals` was called with empty `deals` array.
        NoProposalsToBePublished,
        /// `publish_storage_deals` must be called by Storage Providers and it's a Provider of all of the deals.
        /// This error is emitted when a storage provider tries to publish deals that to not belong to them.
        ProposalsPublishedByIncorrectStorageProvider,
        /// `publish_storage_deals`'s core logic was invoked with a broken invariant that should be called by `validate_deals`.
        UnexpectedValidationError,
        /// There is more than 1 deal of this ID in the Sector.
        DuplicateDeal,
        /// Due to a programmer bug, bounds on Bounded data structures were incorrect so couldn't insert into them.
        DealPreconditionFailed,
        /// Sum of all of the deals piece sizes for a sector exceeds sector size.
        DealsTooLargeToFitIntoSector,
        /// Tried to activate too many deals at a given start_block.
        TooManyDealsPerBlock,
        /// Try to call an operation as a storage provider but the account is not registered as a storage provider.
        StorageProviderNotRegistered,
        /// CommD related error
        CommD,
        /// Tried to propose a deal, but there are too many pending deals.
        /// The pending deals should be activated or wait for expiry.
        TooManyPendingDeals,
        /// Deal was tried to be activated by a provider which does not own it
        InvalidProvider,
        /// Deal should have been activated earlier, it's too late
        StartBlockElapsed,
        /// Sector containing the deal will expire before the deal is supposed to end
        SectorExpiresBeforeDeal,
        /// Deal needs to be [`DealState::Published`] if it's to be activated
        InvalidDealState,
        /// Tried to activate a deal which is not in the Pending Proposals
        DealNotPending,
        /// Deal was not found in the [`Proposals`] table.
        DealNotFound,
        /// Caller is not the provider.
        InvalidCaller,
        /// Deal is not active
        DealIsNotActive,
        /// ClientDealProposal.client_signature did not match client's public key and data.
        WrongClientSignatureOnProposal,
        /// Deal's block_start > block_end, so it doesn't make sense.
        DealEndBeforeStart,
        /// Deal's start block is in the past, it should be in the future.
        DealStartExpired,
        /// Deal has to be [`DealState::Published`] when being Published
        DealNotPublished,
        /// Deal's duration must be within `Config::MinDealDuration` < `Config:MaxDealDuration`.
        DealDurationOutOfBounds,
        /// Deal's piece_cid is invalid.
        InvalidPieceCid,
        /// The proposed deal parameters' falls outside the parameter bounds set by the Storage Provider.
        OutOfBoundsDeal,
        /// The SP attempted to remove DealParameters while there are none present.
        NoDealParamsToRemove,
        /// The deal parameters that the SP submitted are not valid
        InvalidDealParametersSubmitted,
        /// Emitted when a storage provider is trying to be registered
        /// but there is already storage provider registered for that `AccountId`.
        StorageProviderExists,
        /// Emitted when a type conversion fails.
        ConversionError,
        /// Emitted when an account tries to call a storage provider
        /// extrinsic but is not registered as one.
        StorageProviderNotFound,
        /// Emitted when trying to access an invalid sector.
        InvalidSector,
        /// Emitted when trying to submit PoSt for an not-existing partition for a deadline.
        InvalidPartition,
        /// Emitted when submitting an invalid proof type.
        InvalidProofType,
        /// Emitted when the proof is invalid
        InvalidProof,
        /// Emitted when there is not enough funds to run an extrinsic.
        NotEnoughFunds,
        /// Emitted when trying to reuse a sector number
        SectorNumberAlreadyUsed,
        /// Emitted when expiration is after activation
        ExpirationBeforeActivation,
        /// Emitted when expiration is less than minimum after activation
        ExpirationTooSoon,
        /// Emitted when the expiration exceeds MaxSectorExpiration
        ExpirationTooLong,
        /// Emitted when a sectors lifetime exceeds SectorMaximumLifetime
        MaxSectorLifetimeExceeded,
        /// Emitted when a CID is invalid
        InvalidCid,
        /// Emitted when a prove commit is sent after the deadline.
        /// These pre-commits will be cleaned up in the hook.
        ProveCommitAfterDeadline,
        /// Emitted when a PoSt supplied by by the SP is invalid
        PoStProofInvalid,
        /// Emitted when an error occurs when submitting PoSt.
        InvalidDeadlineSubmission,
        /// Emitted when Market::verify_deals_for_activation fails for an unexpected reason.
        /// Verification happens in pre_commit, to make sure a sector is precommited with valid deals.
        CouldNotVerifySectorForPreCommit,
        /// Declared unsealed_cid for pre_commit is different from the one calculated by `Market::verify_deals_for_activation`.
        /// unsealed_cid === CommD and is calculated from piece ids of all of the deals in a sector.
        InvalidUnsealedCidForSector,
        /// Emitted when SP calls declare_faults and the fault cutoff is passed.
        FaultDeclarationTooLate,
        /// Emitted when SP calls declare_faults_recovered and the fault recovery cutoff is passed.
        FaultRecoveryTooLate,
        /// Tried to slash reserved currency and burn it.
        SlashingFailed,
        /// Emitted when trying to terminate sector deals fails.
        CouldNotTerminateDeals,
        /// Tried to terminate sectors that are not mutable.
        CannotTerminateImmutableDeadline,
        /// Emitted when trying to submit PoSt with partitions containing too many sectors (>2349).
        TooManyReplicas,
        /// SubmitWindowedPoSt must accept the same number of proofs as ProofVerification trait.
        /// Internal error, should not happen.
        TooManyProofs,
        /// AuthorVRF lookup failed.
        MissingAuthorVRF,
        /// After proving failed to return pre commit deposit.
        FailedToReturnPreCommitDeposit,
        /// When an SP tries to deregister while it still has active deals.
        SPHasActiveDeals,
        /// When an SP tries to deregister while it still has pre committed sectors.
        SPHasPreCommittedSectors,
        /// Inner pallet errors
        GeneralPalletError(crate::error::GeneralPalletError),
    }

    impl<T> From<crate::error::GeneralPalletError> for Error<T> {
        fn from(err: crate::error::GeneralPalletError) -> Error<T> {
            Error::<T>::GeneralPalletError(err)
        }
    }

    #[pallet::composite_enum]
    pub enum HoldReason {
        ClientDealFee,
        ProviderDealCollateral,
        ProviderPreCommitDeposit,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight((T::WeightInfo::register_storage_provider(), DispatchClass::Normal))]
        pub fn register_storage_provider(
            origin: OriginFor<T>,
            multiaddr: T::Multiaddr,
            window_post_proof_type: RegisteredPoStProof,
        ) -> DispatchResult {
            crate::dispatchables::register_storage_provider::<T>(
                origin,
                multiaddr,
                window_post_proof_type,
            )
        }

        #[pallet::call_index(1)]
        #[pallet::weight((T::WeightInfo::publish_deal_parameters(2), DispatchClass::Normal))]
        pub fn publish_deal_parameters(
            origin: OriginFor<T>,
            deal_parameters: OffchainDealParameters<BalanceOf<T>, BlockNumberFor<T>>,
        ) -> DispatchResult {
            crate::dispatchables::publish_deal_parameters::<T>(origin, deal_parameters)
        }

        #[pallet::call_index(2)]
        #[pallet::weight((T::WeightInfo::remove_deal_parameters(), DispatchClass::Normal))]
        pub fn remove_deal_parameters(origin: OriginFor<T>) -> DispatchResult {
            crate::dispatchables::remove_deal_parameters::<T>(origin)
        }

        /// Publish a new set of storage deals (not yet included in a sector).
        /// It saves valid deals as [`DealState::Published`] and locks up client fees and provider's collaterals.
        /// Locked up balances cannot be withdrawn until a deal is terminated.
        /// All of the deals must belong to a single Storage Provider.
        /// It is permissive, if some of the deals are correct and some are not, it emits events for valid deals.
        /// On success emits [`Event::<T>::DealPublished`] for each successful deal.
        #[pallet::call_index(3)]
        #[pallet::weight((T::WeightInfo::publish_storage_deals(deals.len() as u32), DispatchClass::Normal))]
        pub fn publish_storage_deals(
            origin: OriginFor<T>,
            deals: BoundedVec<
                ClientDealProposal<
                    T::AccountId,
                    BalanceOf<T>,
                    BlockNumberFor<T>,
                    T::OffchainSignature,
                >,
                T::MaxDeals,
            >,
        ) -> DispatchResult {
            crate::dispatchables::publish_storage_deals::<T>(origin, deals)
        }

        /// The Storage Provider uses this extrinsic to pledge and seal X sectors at once.
        /// If a single sector fails to pre commit for whatever reason, the extrinsic will fail.
        ///
        /// The deposit amount is calculated by `calculate_pre_commit_deposit`.
        /// The deposited amount is locked until the sector has been terminated.
        /// A hook will check pre-committed sectors `expiration` and
        /// if that sector has not been proven by that time the deposit will be slashed.
        /// Reference implementation:
        /// * <https://github.com/filecoin-project/builtin-actors/blob/6906288334746318385cfd53edd7ea33ef03919f/actors/miner/src/lib.rs#L1453>
        #[pallet::call_index(4)]
        #[pallet::weight((T::WeightInfo::pre_commit_sectors(), DispatchClass::Normal))]
        pub fn pre_commit_sectors(
            origin: OriginFor<T>,
            sectors: BoundedVec<
                SectorPreCommitInfo<BlockNumberFor<T>>,
                ConstU32<MAX_SECTORS_PER_CALL>,
            >,
        ) -> DispatchResult {
            crate::dispatchables::pre_commit_sectors::<T>(origin, sectors)
        }

        /// Allows the storage providers to submit proof for their pre-committed
        /// sectors.
        #[pallet::call_index(5)]
        #[pallet::weight((T::WeightInfo::prove_commit_sectors(), DispatchClass::Normal))]
        pub fn prove_commit_sectors(
            origin: OriginFor<T>,
            sectors: BoundedVec<ProveCommitSector, ConstU32<MAX_SECTORS_PER_CALL>>,
        ) -> DispatchResult {
            crate::dispatchables::prove_commit_sectors::<T>(origin, sectors)
        }

        /// The SP uses this extrinsic to submit their Proof-of-Spacetime.
        #[pallet::call_index(6)]
        #[pallet::weight((T::WeightInfo::submit_windowed_post(), DispatchClass::Normal))]
        pub fn submit_windowed_post(
            origin: OriginFor<T>,
            windowed_post: SubmitWindowedPoStParams,
        ) -> DispatchResult {
            crate::dispatchables::submit_windowed_post::<T>(origin, windowed_post)
        }

        /// The SP uses this extrinsic to declare some sectors as faulty. Letting the system know it will not submit PoSt for the next deadline.
        ///
        /// References:
        /// * <https://github.com/filecoin-project/builtin-actors/blob/82d02e58f9ef456aeaf2a6c737562ac97b22b244/actors/miner/src/lib.rs#L2648>
        #[pallet::call_index(7)]
        #[pallet::weight((T::WeightInfo::declare_faults(), DispatchClass::Normal))]
        pub fn declare_faults(origin: OriginFor<T>, params: DeclareFaultsParams) -> DispatchResult {
            crate::dispatchables::declare_faults::<T>(origin, params)
        }

        /// This extrinsic allows an SP to declare some faulty sectors as recovering.
        /// Sectors can either be declared faulty by the SP or by the system.
        /// The system declares a sector as faulty when an SP misses their PoSt deadline.
        ///
        /// References:
        /// * <https://github.com/filecoin-project/builtin-actors/blob/0f205c378983ac6a08469b9f400cbb908eef64e2/actors/miner/src/lib.rs#L2620>
        #[pallet::call_index(8)]
        #[pallet::weight((T::WeightInfo::declare_faults_recovered(), DispatchClass::Normal))]
        pub fn declare_faults_recovered(
            origin: OriginFor<T>,
            params: DeclareFaultsRecoveredParams,
        ) -> DispatchResult {
            crate::dispatchables::declare_faults_recovered::<T>(origin, params)
        }

        /// Marks some sectors as terminated at the present block, earlier than their
        /// scheduled termination, and adds these sectors to the early termination queue.
        ///
        /// References:
        /// * https://github.com/filecoin-project/builtin-actors/blob/8d957d2901c0f2044417c268f0511324f591cb92/actors/miner/src/lib.rs#L2488-L2505
        #[pallet::call_index(9)]
        #[pallet::weight((T::WeightInfo::terminate_sectors(), DispatchClass::Normal))]
        pub fn terminate_sectors(
            origin: OriginFor<T>,
            params: TerminateSectorsParams,
        ) -> DispatchResult {
            crate::dispatchables::terminate_sectors::<T>(origin, params)
        }

        /// Settle pending deal payments for the given deal IDs.
        ///
        /// This function *should* only fully fail when a block was last updated after its `end_block` target.
        ///
        /// In other cases, the function will return two lists, the successful settlements and the unsuccessful ones.
        ///
        /// A settlement is only fully performed when a deal is active.
        ///
        /// A settlement is unsuccessful when:
        /// * The deal was not found. The returned error is [`DealSettlementError::DealNotFound`].
        /// * The deal's start block is after the current block, meaning it's too early to settle the deal.
        ///   The returned error is [`DealSettlementError::EarlySettlement`].
        /// * The deal has been slashed. The returned error is [`DealSettlementError::SlashedDeal`].
        /// * The deal's last update is after the current block, meaning the deal's last update is in the future.
        ///   The returned error is [`DealSettlementError::FutureLastUpdate`].
        /// * The deal is not active
        #[pallet::call_index(10)]
        #[pallet::weight((T::WeightInfo::settle_deal_payments(deal_ids.len() as u32), DispatchClass::Normal))]
        pub fn settle_deal_payments(
            origin: OriginFor<T>,
            // The original `deals` structure is a bitfield from fvm-ipld-bitfield
            deal_ids: BoundedVec<DealId, T::MaxDeals>,
        ) -> DispatchResult {
            crate::dispatchables::settle_deal_payments::<T>(origin, deal_ids)
        }

        #[pallet::call_index(13)]
        #[pallet::weight((T::WeightInfo::deregister_storage_provider(), DispatchClass::Normal))]
        pub fn deregister_storage_provider(origin: OriginFor<T>) -> DispatchResult {
            crate::dispatchables::deregister_storage_provider::<T>(origin)
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
            // TODO(@th7nder,#77,26/06/2024): set proper weights according to what does the `on_finalize` do
            // return placeholder for now
            // the correct way: get number of deals for a given block from DealsForBlock
            // and then calculate weights according to the actions performed in on_finalize
            T::DbWeight::get().reads(1)
        }

        fn on_finalize(current_block: BlockNumberFor<T>) {
            crate::hooks::check_precommited_sectors::<T>(current_block);
            crate::hooks::check_deadlines::<T>(current_block);
            crate::hooks::slash_providers::<T>(current_block);
        }
    }

    // impl<T: Config> StorageProviderValidation<T::AccountId> for Pallet<T> {
    //     fn is_registered_storage_provider(storage_provider: &T::AccountId) -> bool {
    //         StorageProviders::<T>::contains_key(storage_provider)
    //     }
    // }

    impl<T: Config> Pallet<T> {
        /// Account Id of the pallet
        ///
        /// This actually does computation.
        /// If you need to keep using it, make sure you cache it and call it once.
        pub fn account_id() -> T::AccountId {
            T::PalletId::get().into_account_truncating()
        }

        /// Used for deduplication purposes
        /// We don't want to store another BTreeSet of DealProposals
        /// We only care about hashes.
        /// It is not an associated function, because T::Hashing is hard to use inside of there.
        pub fn hash_proposal(
            proposal: &DealProposal<T::AccountId, BalanceOf<T>, BlockNumberFor<T>>,
        ) -> T::Hash {
            let bytes = Encode::encode(proposal);
            T::Hashing::hash(&bytes)
        }

        /// Returns true if the account is a registered storage provider.
        pub fn is_registered_storage_provider(account: &T::AccountId) -> bool {
            StorageProviders::<T>::contains_key(account)
        }

        /// Gets the next, not yet opened, deadline of the storage provider.
        ///
        /// If there is no Storage Provider of given AccountId returns [`Option::None`].
        /// May exceptionally return [`Option::None`] when
        /// conversion between BlockNumbers fails, but technically should never happen.
        pub fn deadline_info(
            storage_provider: &T::AccountId,
            deadline_index: u64,
        ) -> Option<ExternalDeadlineInfo<BlockNumberFor<T>>> {
            let sp = StorageProviders::<T>::try_get(storage_provider).ok()?;
            let current_block = <frame_system::Pallet<T>>::block_number();

            let deadline = DeadlineInfo::new(
                current_block,
                sp.proving_period_start,
                deadline_index,
                T::WPoStPeriodDeadlines::get(),
                T::WPoStProvingPeriod::get(),
                T::WPoStChallengeWindow::get(),
                T::WPoStChallengeLookBack::get(),
                T::FaultDeclarationCutoff::get(),
            )
            .and_then(DeadlineInfo::next_not_opened)
            .ok()?;

            Some(ExternalDeadlineInfo {
                deadline_index: deadline.idx,
                open: deadline.is_open(),
                challenge_block: deadline.challenge,
                start: deadline.open_at,
                close: deadline.close_at,
            })
        }

        /// Returns snapshot information about the deadline, i.e. which sectors are assigned to which partitions.
        /// When the deadline has not opened yet (deadline_start - WPoStChallengeWindow), it can change!
        pub fn deadline_state(
            storage_provider: &T::AccountId,
            deadline_index: u64,
        ) -> Option<primitives::pallets::DeadlineState> {
            let sp = StorageProviders::<T>::try_get(storage_provider).ok()?;
            let deadline_index: usize = deadline_index.try_into().ok()?;

            if deadline_index >= sp.deadlines.due.len() {
                log::warn!(
                    "tried to get non existing deadline: {}/{}",
                    deadline_index,
                    sp.deadlines.due.len()
                );
                return None;
            }

            let deadline = &sp.deadlines.due[deadline_index];
            let mut partitions = BoundedBTreeMap::new();
            for (partition_number, partition) in deadline.partitions.iter() {
                partitions
                    .try_insert(
                        *partition_number,
                        primitives::pallets::PartitionState {
                            sectors: partition.live_sectors(),
                        },
                    )
                    .ok()?;
            }

            Some(primitives::pallets::DeadlineState { partitions })
        }
    }
}
