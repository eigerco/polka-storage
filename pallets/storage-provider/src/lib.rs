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

pub use pallet::*;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    pub(crate) const DECLARATIONS_MAX: u32 = 3000;
    pub(crate) const LOG_TARGET: &'static str = "runtime::storage_provider";

    extern crate alloc;

    use alloc::vec;
    use core::fmt::Debug;

    use codec::{Decode, Encode};
    use frame_support::{dispatch::DispatchResult, pallet_prelude::*, traits::Randomness};
    use frame_system::pallet_prelude::{BlockNumberFor, *};
    use primitives::{
        configs::{BalanceOf, CurrencyProvider, StorageProviderProvider},
        pallets::{
            DeadlineInfo as ExternalDeadlineInfo, Market, ProofVerification,
            StorageProviderValidation,
        },
        proofs::RegisteredPoStProof,
        randomness::AuthorVrfHistory,
        sector::{ProveCommitSector, SectorNumber, SectorPreCommitInfo},
        PartitionNumber, MAX_PARTITIONS_PER_DEADLINE, MAX_SECTORS, MAX_SECTORS_PER_CALL,
    };
    use scale_info::TypeInfo;

    use crate::{
        deadline::DeadlineInfo,
        fault::{
            DeclareFaultsParams, DeclareFaultsRecoveredParams, FaultDeclaration,
            RecoveryDeclaration,
        },
        proofs::SubmitWindowedPoStParams,
        sector::{ProveCommitResult, TerminateSectorsParams, TerminationDeclaration},
        storage_provider::{StorageProviderInfo, StorageProviderState},
    };

    #[pallet::pallet]
    #[pallet::without_storage_info] // Allows to define storage items without fixed size
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config + CurrencyProvider + StorageProviderProvider {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Randomness generator
        type Randomness: Randomness<Self::Hash, BlockNumberFor<Self>>;

        /// Peer ID is derived by hashing an encoded public key.
        /// Usually represented in bytes.
        /// https://github.com/libp2p/specs/blob/2ea41e8c769f1bead8e637a9d4ebf8c791976e8a/peer-ids/peer-ids.md#peer-ids
        /// More information about libp2p peer ids: https://docs.libp2p.io/concepts/fundamentals/peers/
        type PeerId: Clone + Debug + Decode + Encode + Eq + TypeInfo;

        /// Market trait implementation for activating deals.
        type Market: Market<Self::AccountId, BlockNumberFor<Self>, BalanceOf<Self>>;

        /// Proof verification trait implementation for verifying proofs.
        type ProofVerification: ProofVerification;

        /// Trait for AuthorVRF querying.
        type AuthorVrfHistory: AuthorVrfHistory<BlockNumberFor<Self>, Self::Hash>;

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
        StorageProviderState<T::PeerId, BalanceOf<T>, BlockNumberFor<T>>,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Emitted when a new storage provider is registered.
        StorageProviderRegistered {
            owner: T::AccountId,
            info: StorageProviderInfo<T::PeerId>,
            proving_period_start: BlockNumberFor<T>,
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
        /// Inner pallet errors
        GeneralPalletError(crate::error::GeneralPalletError),
    }

    impl<T> From<crate::error::GeneralPalletError> for Error<T> {
        fn from(err: crate::error::GeneralPalletError) -> Error<T> {
            Error::<T>::GeneralPalletError(err)
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        pub fn register_storage_provider(
            origin: OriginFor<T>,
            peer_id: T::PeerId,
            window_post_proof_type: RegisteredPoStProof,
        ) -> DispatchResult {
            crate::dispatchables::register_storage_provider::<T>(
                origin,
                peer_id,
                window_post_proof_type,
            )
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
        #[pallet::call_index(1)]
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
        #[pallet::call_index(2)]
        pub fn prove_commit_sectors(
            origin: OriginFor<T>,
            sectors: BoundedVec<ProveCommitSector, ConstU32<MAX_SECTORS_PER_CALL>>,
        ) -> DispatchResult {
            crate::dispatchables::prove_commit_sectors::<T>(origin, sectors)
        }

        /// The SP uses this extrinsic to submit their Proof-of-Spacetime.
        #[pallet::call_index(3)]
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
        #[pallet::call_index(4)]
        pub fn declare_faults(origin: OriginFor<T>, params: DeclareFaultsParams) -> DispatchResult {
            crate::dispatchables::declare_faults::<T>(origin, params)
        }

        /// This extrinsic allows an SP to declare some faulty sectors as recovering.
        /// Sectors can either be declared faulty by the SP or by the system.
        /// The system declares a sector as faulty when an SP misses their PoSt deadline.
        ///
        /// References:
        /// * <https://github.com/filecoin-project/builtin-actors/blob/0f205c378983ac6a08469b9f400cbb908eef64e2/actors/miner/src/lib.rs#L2620>
        #[pallet::call_index(5)]
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
        #[pallet::call_index(6)]
        pub fn terminate_sectors(
            origin: OriginFor<T>,
            params: TerminateSectorsParams,
        ) -> DispatchResult {
            crate::dispatchables::terminate_sectors::<T>(origin, params)
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
            // TODO(@th7nder, no-ref, 2024/07/31): set proper weights
            T::DbWeight::get().reads(1)
        }

        fn on_finalize(current_block: BlockNumberFor<T>) {
            crate::hooks::check_precommited_sectors::<T>(current_block);
            crate::hooks::check_deadlines::<T>(current_block);
        }
    }

    impl<T: Config> StorageProviderValidation<T::AccountId> for Pallet<T> {
        fn is_registered_storage_provider(storage_provider: &T::AccountId) -> bool {
            StorageProviders::<T>::contains_key(storage_provider)
        }
    }

    impl<T: Config> Pallet<T> {
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
