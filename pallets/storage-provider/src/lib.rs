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
pub use pallet::{Config, Pallet};

#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;

#[cfg(test)]
mod tests;

mod deadline;
mod fault;
mod partition;
mod proofs;
mod sector;
mod sector_map;
mod storage_provider;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    pub const CID_CODEC: u64 = 0x55;
    /// Sourced from multihash code table <https://github.com/multiformats/rust-multihash/blob/b321afc11e874c08735671ebda4d8e7fcc38744c/codetable/src/lib.rs#L108>
    pub const BLAKE2B_MULTIHASH_CODE: u64 = 0xB220;
    pub(crate) const DECLARATIONS_MAX: u32 = 3000;
    const LOG_TARGET: &'static str = "runtime::storage_provider";

    extern crate alloc;

    use alloc::{vec, vec::Vec};
    use core::fmt::Debug;

    use cid::{Cid, Version};
    use codec::{Decode, Encode};
    use frame_support::{
        dispatch::DispatchResult,
        ensure, fail,
        pallet_prelude::*,
        sp_runtime::traits::{CheckedAdd, CheckedSub},
        traits::{
            Currency, ExistenceRequirement::KeepAlive, Imbalance, ReservableCurrency,
            WithdrawReasons,
        },
    };
    use frame_system::{
        ensure_signed,
        pallet_prelude::{BlockNumberFor, *},
        Config as SystemConfig,
    };
    use primitives_proofs::{Market, RegisteredPoStProof, RegisteredSealProof, SectorNumber};
    use scale_info::TypeInfo;
    use sp_arithmetic::traits::Zero;

    use crate::{
        deadline::DeadlineInfo,
        fault::{DeclareFaultsParams, FaultDeclaration},
        proofs::{assign_proving_period_offset, SubmitWindowedPoStParams},
        sector::{
            ProveCommitSector, SectorOnChainInfo, SectorPreCommitInfo, SectorPreCommitOnChainInfo,
            MAX_SECTORS,
        },
        sector_map::DeadlineSectorMap,
        storage_provider::{
            calculate_first_proving_period_start, calculate_proving_period_start_with_seed,
            StorageProviderInfo, StorageProviderState,
        },
    };

    /// Allows to extract Balance of an account via the Config::Currency associated type.
    /// BalanceOf is a sophisticated way of getting an u128.
    type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as SystemConfig>::AccountId>>::Balance;

    #[pallet::pallet]
    #[pallet::without_storage_info] // Allows to define storage items without fixed size
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Peer ID is derived by hashing an encoded public key.
        /// Usually represented in bytes.
        /// https://github.com/libp2p/specs/blob/2ea41e8c769f1bead8e637a9d4ebf8c791976e8a/peer-ids/peer-ids.md#peer-ids
        /// More information about libp2p peer ids: https://docs.libp2p.io/concepts/fundamentals/peers/
        type PeerId: Clone + Debug + Decode + Encode + Eq + TypeInfo;

        /// Currency mechanism, used for collateral
        type Currency: ReservableCurrency<Self::AccountId>;

        /// Market trait implementation for activating deals
        type Market: Market<Self::AccountId, BlockNumberFor<Self>>;

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
        type MaxSectorExpirationExtension: Get<BlockNumberFor<Self>>;

        /// Maximum number of blocks a sector can stay in pre-committed state
        #[pallet::constant]
        type SectorMaximumLifetime: Get<BlockNumberFor<Self>>;

        /// Maximum duration to allow for the sealing process for seal algorithms.
        #[pallet::constant]
        type MaxProveCommitDuration: Get<BlockNumberFor<Self>>;

        /// Represents how many challenge deadline there are in 1 proving period.
        /// Closely tied to `WPoStChallengeWindow`
        #[pallet::constant]
        type WPoStPeriodDeadlines: Get<u64>;

        #[pallet::constant]
        type MaxPartitionsPerDeadline: Get<u64>;

        /// The longest a faulty sector can live without being removed.
        #[pallet::constant]
        type FaultMaxAge: Get<BlockNumberFor<Self>>;
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
    #[pallet::generate_deposit(fn deposit_event)]
    pub enum Event<T: Config> {
        /// Emitted when a new storage provider is registered.
        StorageProviderRegistered {
            owner: T::AccountId,
            info: StorageProviderInfo<T::PeerId>,
        },
        /// Emitted when a storage provider pre commits some sectors.
        SectorPreCommitted {
            owner: T::AccountId,
            sector: SectorPreCommitInfo<BlockNumberFor<T>>,
        },
        /// Emitted when a storage provider successfully proves pre committed sectors.
        SectorProven {
            owner: T::AccountId,
            sector_number: SectorNumber,
        },
        /// Emitted when a sector was pre-committed, but not proven, so it got slashed in the pre-commit hook.
        SectorSlashed {
            owner: T::AccountId,
            sector_number: SectorNumber,
        },
        /// Emitted when an SP submits a valid PoSt
        ValidPoStSubmitted { owner: T::AccountId },
        /// Emitted when an SP declares some sectors as faulty
        FaultsDeclared {
            owner: T::AccountId,
            faults: BoundedVec<FaultDeclaration, ConstU32<DECLARATIONS_MAX>>,
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
        /// Emitted when submitting an invalid proof type.
        InvalidProofType,
        /// Emitted when there is not enough funds to run an extrinsic.
        NotEnoughFunds,
        /// Emitted when a sector fails to activate.
        SectorActivateFailed,
        /// Emitted when removing a pre_committed sector after proving fails.
        CouldNotRemoveSector,
        /// Emitted when trying to reuse a sector number
        SectorNumberAlreadyUsed,
        /// Emitted when expiration is after activation
        ExpirationBeforeActivation,
        /// Emitted when expiration is less than minimum after activation
        ExpirationTooSoon,
        /// Emitted when the expiration exceeds MaxSectorExpirationExtension
        ExpirationTooLong,
        /// Emitted when a sectors lifetime exceeds SectorMaximumLifetime
        MaxSectorLifetimeExceeded,
        /// Emitted when a CID is invalid
        InvalidCid,
        /// Emitted when a sector fails to activate
        CouldNotActivateSector,
        /// Emitted when a prove commit is sent after the deadline.
        /// These pre-commits will be cleaned up in the hook.
        ProveCommitAfterDeadline,
        /// Emitted when a PoSt supplied by by the SP is invalid
        PoStProofInvalid,
        /// Emitted when an error occurs when submitting PoSt.
        InvalidDeadlineSubmission,
        /// Wrapper around the [`DeadlineError`] type.
        DeadlineError(crate::deadline::DeadlineError),
        /// Wrapper around the [`PartitionError`] type.
        PartitionError(crate::partition::PartitionError),
        /// Wrapper around the [`StorageProviderError`] type.
        StorageProviderError(crate::storage_provider::StorageProviderError),
        /// Wrapper around the [`SectorMapError`] type.
        SectorMapError(crate::sector_map::SectorMapError),
        /// Emitted when Market::verify_deals_for_activation fails for an unexpected reason.
        /// Verification happens in pre_commit, to make sure a sector is precommited with valid deals.
        CouldNotVerifySectorForPreCommit,
        /// Declared unsealed_cid for pre_commit is different from the one calculated by `Market::verify_deals_for_activation`.
        /// unsealed_cid === CommD and is calculated from piece ids of all of the deals in a sector.
        InvalidUnsealedCidForSector,
        /// Emitted when SP calls declare_faults and the fault cutoff is passed.
        FaultDeclarationTooLate,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        pub fn register_storage_provider(
            origin: OriginFor<T>,
            peer_id: T::PeerId,
            window_post_proof_type: RegisteredPoStProof,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            // Ensure that the storage provider does not exist yet
            ensure!(
                !StorageProviders::<T>::contains_key(&owner),
                Error::<T>::StorageProviderExists
            );
            let current_block = <frame_system::Pallet<T>>::block_number();
            let proving_period = T::WPoStProvingPeriod::get();

            let offset = assign_proving_period_offset::<T::AccountId, BlockNumberFor<T>>(
                &owner,
                current_block,
                T::WPoStProvingPeriod::get(),
            )
            .map_err(|_| Error::<T>::ConversionError)?;

            let local_proving_start = calculate_first_proving_period_start::<BlockNumberFor<T>>(
                current_block,
                offset,
                proving_period,
            );
            let info = StorageProviderInfo::new(peer_id, window_post_proof_type);
            let state = StorageProviderState::new(
                info.clone(),
                local_proving_start,
                // Always zero since we're calculating the absolute first start
                // thus the deadline will always be zero
                0,
                T::WPoStPeriodDeadlines::get(),
            );
            StorageProviders::<T>::insert(&owner, state);
            // Emit event
            Self::deposit_event(Event::StorageProviderRegistered { owner, info });
            Ok(())
        }

        /// The Storage Provider uses this extrinsic to pledge and seal a new sector.
        ///
        /// The deposit amount is calculated by `calculate_pre_commit_deposit`.
        /// The deposited amount is locked until the sector has been terminated.
        /// A hook will check pre-committed sectors `expiration` and
        /// if that sector has not been proven by that time the deposit will be slashed.
        // TODO(@aidan46, #107, 2024-06-20): Add functionality to allow for batch pre commit
        pub fn pre_commit_sector(
            origin: OriginFor<T>,
            sector: SectorPreCommitInfo<BlockNumberFor<T>>,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            let sp = StorageProviders::<T>::try_get(&owner)
                .map_err(|_| Error::<T>::StorageProviderNotFound)?;
            let sector_number = sector.sector_number;
            let current_block = <frame_system::Pallet<T>>::block_number();

            ensure!(
                sector_number <= MAX_SECTORS.into(),
                Error::<T>::InvalidSector
            );
            ensure!(
                sp.info.window_post_proof_type == sector.seal_proof.registered_window_post_proof(),
                Error::<T>::InvalidProofType
            );
            ensure!(
                !sp.pre_committed_sectors.contains_key(&sector_number)
                    && !sp.sectors.contains_key(&sector_number),
                Error::<T>::SectorNumberAlreadyUsed
            );

            let unsealed_cid = validate_cid::<T>(&sector.unsealed_cid[..])?;
            let balance = T::Currency::total_balance(&owner);
            let deposit = calculate_pre_commit_deposit::<T>();
            Self::validate_expiration(
                current_block,
                current_block + T::MaxProveCommitDuration::get(),
                sector.expiration,
            )?;
            ensure!(balance >= deposit, Error::<T>::NotEnoughFunds);

            let sector_on_chain = SectorPreCommitOnChainInfo::new(
                sector.clone(),
                deposit,
                <frame_system::Pallet<T>>::block_number(),
            );

            let mut sector_deals = BoundedVec::new();
            sector_deals.try_push((&sector_on_chain).into())
                .map_err(|_| {
                    log::error!(target: LOG_TARGET, "pre_commit_sector: failed to push into sector deals, shouldn't ever happen");
                    Error::<T>::CouldNotVerifySectorForPreCommit
                })?;
            let calculated_commds = T::Market::verify_deals_for_activation(&owner, sector_deals)?;

            ensure!(calculated_commds.len() == 1, {
                log::error!(target: LOG_TARGET, "pre_commit_sector: failed to verify deals, invalid calculated_commd length: {}", calculated_commds.len());
                Error::<T>::CouldNotVerifySectorForPreCommit
            });

            // We need to verify CommD only if there are deals in the sector, otherwise it's a Committed Capacity sector.
            if sector.deal_ids.len() > 0 {
                // PRE-COND: verify_deals_for_activation is called with a single sector, so a single CommD should always be returned
                let Some(calculated_commd) = calculated_commds[0] else {
                    log::error!(target: LOG_TARGET, "pre_commit_sector: commd from verify_deals is None...");
                    fail!(Error::<T>::CouldNotVerifySectorForPreCommit)
                };

                ensure!(calculated_commd == unsealed_cid, {
                    log::error!(target: LOG_TARGET, "pre_commit_sector: calculated_commd != sector.unsealed_cid, {:?} != {:?}", calculated_commd, unsealed_cid);
                    Error::<T>::InvalidUnsealedCidForSector
                });
            }

            T::Currency::reserve(&owner, deposit)?;
            StorageProviders::<T>::try_mutate(&owner, |maybe_sp| -> DispatchResult {
                let sp = maybe_sp
                    .as_mut()
                    .ok_or(Error::<T>::StorageProviderNotFound)?;
                sp.add_pre_commit_deposit(deposit)?;
                sp.put_pre_committed_sector(sector_on_chain)
                    .map_err(|e| Error::<T>::StorageProviderError(e))?;
                Ok(())
            })?;
            Self::deposit_event(Event::SectorPreCommitted { owner, sector });
            Ok(())
        }

        /// Allows the storage providers to submit proof for their pre-committed sectors.
        // TODO(@aidan46, no-ref, 2024-06-24): Add functionality to allow for batch pre commit
        // TODO(@aidan46, no-ref, 2024-06-24): Actually check proof, currently the proof validation is stubbed out.
        pub fn prove_commit_sector(
            origin: OriginFor<T>,
            sector: ProveCommitSector,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            let sp = StorageProviders::<T>::try_get(&owner)
                .map_err(|_| Error::<T>::StorageProviderNotFound)?;
            let sector_number = sector.sector_number;
            ensure!(
                sector_number <= MAX_SECTORS.into(),
                Error::<T>::InvalidSector
            );

            let precommit = sp
                .get_pre_committed_sector(sector_number)
                .map_err(|e| Error::<T>::StorageProviderError(e))?;
            let current_block = <frame_system::Pallet<T>>::block_number();
            let prove_commit_due =
                precommit.pre_commit_block_number + T::MaxProveCommitDuration::get();
            ensure!(
                current_block < prove_commit_due,
                Error::<T>::ProveCommitAfterDeadline
            );
            ensure!(
                validate_seal_proof(&precommit.info.seal_proof, sector.proof),
                Error::<T>::InvalidProofType,
            );

            let new_sector =
                SectorOnChainInfo::from_pre_commit(precommit.info.clone(), current_block);

            StorageProviders::<T>::try_mutate(&owner, |maybe_sp| -> DispatchResult {
                let sp = maybe_sp
                    .as_mut()
                    .ok_or(Error::<T>::StorageProviderNotFound)?;
                sp.activate_sector(sector_number, new_sector.clone())
                    .map_err(|e| Error::<T>::StorageProviderError(e))?;

                let mut new_sectors = BoundedVec::new();
                new_sectors
                    .try_push(new_sector)
                    .expect("Infallible since only 1 element is inserted");

                sp.assign_sectors_to_deadlines(
                    current_block,
                    new_sectors,
                    sp.info.window_post_partition_sectors,
                    T::MaxPartitionsPerDeadline::get(),
                    T::FaultMaxAge::get(),
                    T::WPoStPeriodDeadlines::get(),
                    T::WPoStProvingPeriod::get(),
                    T::WPoStChallengeWindow::get(),
                    T::WPoStChallengeLookBack::get(),
                )
                .map_err(|e| Error::<T>::StorageProviderError(e))?;

                sp.remove_pre_committed_sector(sector_number)
                    .map_err(|e| Error::<T>::StorageProviderError(e))?;
                Ok(())
            })?;

            let mut sector_deals = BoundedVec::new();
            sector_deals
                .try_push(precommit.into())
                .map_err(|_| Error::<T>::CouldNotActivateSector)?;

            let deal_amount = sector_deals.len();
            T::Market::activate_deals(&owner, sector_deals, deal_amount > 0)?;

            Self::deposit_event(Event::SectorProven {
                owner,
                sector_number,
            });

            Ok(())
        }

        /// The SP uses this extrinsic to submit their Proof-of-Spacetime.
        ///
        /// * Proofs are checked with `validate_windowed_post`.
        /// * Currently the proof is considered valid when `proof.len() > 0`.
        pub fn submit_windowed_post(
            origin: OriginFor<T>,
            windowed_post: SubmitWindowedPoStParams<BlockNumberFor<T>>,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            let current_block = <frame_system::Pallet<T>>::block_number();
            let sp = StorageProviders::<T>::try_get(&owner)
                .map_err(|_| Error::<T>::StorageProviderNotFound)?;

            if let Err(e) = Self::validate_windowed_post(
                current_block,
                &windowed_post,
                sp.info.window_post_proof_type,
            ) {
                log::error!(target: LOG_TARGET, "submit_window_post: PoSt submission is invalid {e:?}");
                return Err(e.into());
            }

            // If the proving period is in the future, we can't submit a proof yet
            // Related issue: https://github.com/filecoin-project/specs-actors/issues/946
            ensure!(current_block >= sp.proving_period_start, {
                log::error!(target: LOG_TARGET,
                    "proving period hasn't opened yet (current_block: {:?}, proving_period_start: {:?})",
                    current_block,
                    sp.proving_period_start
                );
                Error::<T>::InvalidDeadlineSubmission
            });

            let current_deadline = sp
                .deadline_info(
                    current_block,
                    T::FaultMaxAge::get(),
                    T::WPoStPeriodDeadlines::get(),
                    T::WPoStProvingPeriod::get(),
                    T::WPoStChallengeWindow::get(),
                    T::WPoStChallengeLookBack::get(),
                )
                .map_err(|e| Error::<T>::DeadlineError(e))?;

            Self::validate_deadline(&current_deadline, &windowed_post)?;

            // mutate provider state
            StorageProviders::<T>::try_mutate(&owner, |maybe_sp| -> DispatchResult {
                let sp = maybe_sp
                    .as_mut()
                    .ok_or(Error::<T>::StorageProviderNotFound)?;
                let deadlines = sp.get_deadlines_mut();

                // record sector as proven
                deadlines
                    .record_proven(windowed_post.deadline as usize, windowed_post.partition)
                    .map_err(|e| Error::<T>::DeadlineError(e))?;

                Ok(())
            })?;

            log::debug!(target: LOG_TARGET, "submit_windowed_post: proof recorded");

            Self::deposit_event(Event::ValidPoStSubmitted { owner });

            Ok(())
        }

        /// The SP uses this extrinsic to declare some sectors as faulty. Letting the system know it will not submit PoSt for the next deadline.
        ///
        /// Filecoin reference: <https://github.com/filecoin-project/builtin-actors/blob/82d02e58f9ef456aeaf2a6c737562ac97b22b244/actors/miner/src/lib.rs#L2648>
        pub fn declare_faults(origin: OriginFor<T>, params: DeclareFaultsParams) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            let current_block = <frame_system::Pallet<T>>::block_number();
            let mut sp = StorageProviders::<T>::try_get(&owner)
                .map_err(|_| Error::<T>::StorageProviderNotFound)?;

            let mut to_process = DeadlineSectorMap::new();
            for term in params.faults.clone() {
                let deadline = term.deadline;
                let partition = term.partition;

                to_process
                    .try_insert(deadline, partition, term.sectors)
                    .map_err(|e| Error::<T>::SectorMapError(e))?;
            }

            let wpost_proving_period = T::WPoStProvingPeriod::get();
            let proving_period_start = calculate_proving_period_start_with_seed(
                current_block,
                sp.proving_period_start,
                wpost_proving_period,
            );

            let sectors = sp.sectors.clone();
            for (deadline_idx, partition_map) in to_process.into_iter() {
                log::debug!(target: LOG_TARGET, "declare_faults: Processing deadline index: {deadline_idx}");
                // Get the deadline
                let target_dl = DeadlineInfo::new(
                    current_block,
                    proving_period_start,
                    *deadline_idx,
                    T::FaultMaxAge::get(),
                    T::WPoStPeriodDeadlines::get(),
                    T::WPoStChallengeWindow::get(),
                    T::WPoStProvingPeriod::get(),
                    T::WPoStChallengeLookBack::get(),
                )
                .map_err(|e| Error::<T>::DeadlineError(e))?;
                ensure!(!target_dl.fault_cutoff_passed(), {
                    log::error!(target: LOG_TARGET, "declare_faults: Late fault declaration at deadline {deadline_idx}");
                    Error::<T>::FaultDeclarationTooLate
                });
                let fault_expiration_block = target_dl.last() + T::FaultMaxAge::get();
                log::debug!(target: LOG_TARGET, "declare_faults: Getting deadline[{deadline_idx}]");
                let dl = sp
                    .deadlines
                    .load_deadline_mut(*deadline_idx as usize)
                    .map_err(|e| Error::<T>::DeadlineError(e))?;
                dl.record_faults(&sectors, partition_map, fault_expiration_block)
                    .map_err(|e| Error::<T>::DeadlineError(e))?;
            }
            StorageProviders::<T>::insert(owner.clone(), sp);
            Self::deposit_event(Event::FaultsDeclared {
                owner,
                faults: params.faults,
            });
            Ok(())
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
            // TODO(@th7nder, no-ref, 2024/07/31): set proper weights
            T::DbWeight::get().reads(1)
        }

        fn on_finalize(current_block: BlockNumberFor<T>) {
            Self::check_precommited_sectors(current_block);
        }
    }

    impl<T: Config> Pallet<T> {
        fn validate_expiration(
            curr_block: BlockNumberFor<T>,
            activation: BlockNumberFor<T>,
            expiration: BlockNumberFor<T>,
        ) -> Result<(), Error<T>> {
            // Expiration must be after activation. Check this explicitly to avoid an underflow below.
            ensure!(
                expiration >= activation,
                Error::<T>::ExpirationBeforeActivation
            );
            // expiration cannot be less than minimum after activation
            ensure!(
                expiration - activation > T::MinSectorExpiration::get(),
                Error::<T>::ExpirationTooSoon
            );
            // expiration cannot exceed MaxSectorExpirationExtension from now
            ensure!(
                expiration < curr_block + T::MaxSectorExpirationExtension::get(),
                Error::<T>::ExpirationTooLong,
            );
            // total sector lifetime cannot exceed SectorMaximumLifetime for the sector's seal proof
            ensure!(
                expiration - activation < T::SectorMaximumLifetime::get(),
                Error::<T>::MaxSectorLifetimeExceeded
            );
            Ok(())
        }

        /// Validates the SPs submitted PoSt by checking if:
        /// - it has the correct proof type
        /// - the proof length is > 0
        /// - the chain commit block < current block
        fn validate_windowed_post(
            current_block: BlockNumberFor<T>,
            windowed_post: &SubmitWindowedPoStParams<BlockNumberFor<T>>,
            expected_proof: RegisteredPoStProof,
        ) -> Result<(), Error<T>> {
            ensure!(
                windowed_post.proof.post_proof == expected_proof,
                Error::<T>::InvalidProofType
            );
            // TODO(@aidan46, #91, 2024-07-03): Validate the proof after research is done
            ensure!(
                windowed_post.proof.proof_bytes.len() > 0,
                Error::<T>::PoStProofInvalid
            );
            // chain commit block must be less than the current block
            ensure!(
                windowed_post.chain_commit_block < current_block,
                Error::<T>::PoStProofInvalid
            );
            Ok(())
        }

        /// Check whether the given deadline is valid for PoSt submission.
        ///
        /// Fails if:
        /// - The given deadline is not open.
        /// - There is and deadline index mismatch.
        /// - The block the deadline was committed at is after challenge height.
        fn validate_deadline(
            current_deadline: &DeadlineInfo<BlockNumberFor<T>>,
            post_params: &SubmitWindowedPoStParams<BlockNumberFor<T>>,
        ) -> Result<(), Error<T>> {
            // Ensure the deadline is open
            ensure!(current_deadline.is_open(), {
                log::error!(target: LOG_TARGET, "validate_deadline: {current_deadline:?}, deadline isn't open");
                Error::<T>::InvalidDeadlineSubmission
            });

            // Ensure the deadline index matches the one in the post params
            ensure!(post_params.deadline == current_deadline.idx, {
                log::error!(target: LOG_TARGET, "validate_deadline: given index does not match current index {} != {}", post_params.deadline, current_deadline.idx);
                Error::<T>::InvalidDeadlineSubmission
            });

            // Ensure the chain commit block is after or equal the challenge
            // start height
            ensure!(
                post_params.chain_commit_block >= current_deadline.challenge,
                {
                    log::error!(target: LOG_TARGET, "validate_deadline: expected chain_commit_block {:?} to be >= {:?}", post_params.chain_commit_block, current_deadline.challenge);
                    Error::<T>::InvalidDeadlineSubmission
                }
            );
            Ok(())
        }

        /// Goes through all of the registered storage providers and checks if they have any expired pre committed sectors.
        /// If there are any sectors that are expired the total deposit amount for all those sectors will be slashed.
        ///
        /// References:
        /// * <https://github.com/filecoin-project/builtin-actors/blob/82d02e58f9ef456aeaf2a6c737562ac97b22b244/actors/miner/src/state.rs#L1071>
        /// * <https://github.com/filecoin-project/builtin-actors/blob/82d02e58f9ef456aeaf2a6c737562ac97b22b244/actors/miner/src/state.rs#L1054>
        fn check_precommited_sectors(current_block: BlockNumberFor<T>) {
            const LOG_TARGET: &'static str = "runtime::storage_provider::check_precommited_sectors";

            // TODO(@th7nder,31/07/2024): this approach is suboptimal, as it's time complexity is O(StorageProviders * PreCommitedSectors).
            // We can reduce this by indexing pre-committed sectors by BlockNumber in which they're supposed to be activated in PreCommit and remove them in ProveCommit.
            log::info!(target: LOG_TARGET, "checking pre_commited_sectors for block: {:?}", current_block);

            // We cannot modify storage map while inside `iter_keys()` as docs say it's undefined results.
            // And we can use `alloc::Vec`, because it's bounded by StorageProviders data structure anyways.
            let storage_providers: Vec<_> = StorageProviders::<T>::iter_keys().collect();
            for storage_provider in storage_providers {
                log::info!(target: LOG_TARGET, "checking storage provider {:?}", storage_provider);
                let Ok(mut state) = StorageProviders::<T>::try_get(storage_provider.clone()) else {
                    log::error!(target: LOG_TARGET, "catastrophe, couldn't find a storage provider based on key. it should have been there...");
                    continue;
                };

                let (expired, slash_amount) =
                    Self::detect_expired_precommit_sectors(current_block, &state);
                if expired.is_empty() {
                    return;
                }

                log::info!(target: LOG_TARGET, "found {} expired pre committed sectors for {:?}", expired.len(), storage_provider);
                for sector_number in expired {
                    // Expired sectors should be removed, because in other case they'd be processed twice in the next block.
                    let Ok(()) = state.remove_pre_committed_sector(sector_number) else {
                        log::error!(target: LOG_TARGET, "catastrophe, failed to remove sector {} for {:?}", sector_number, storage_provider);
                        continue;
                    };

                    Self::deposit_event(Event::<T>::SectorSlashed {
                        sector_number,
                        owner: storage_provider.clone(),
                    });
                }

                let Some(slashed_deposits) = state.pre_commit_deposits.checked_sub(&slash_amount)
                else {
                    log::error!(target: LOG_TARGET, "catastrophe, failed to subtract from pre_commit_deposits {:?} - {:?} < 0", state.pre_commit_deposits, slash_amount);
                    continue;
                };
                state.pre_commit_deposits = slashed_deposits;

                // PRE-COND: currency was previously reserved in pre_commit
                let (imbalance, balance) =
                    T::Currency::slash_reserved(&storage_provider, slash_amount);
                if balance > BalanceOf::<T>::zero() {
                    log::warn!(target: LOG_TARGET, "could not slash_reserved entirely, invariant violated");
                }
                // slash_reserved returns NegativeImbalance, we need to get a concrete value and burn it to level out the circulating currency
                let imbalance = T::Currency::burn(imbalance.peek());

                let Ok(()) = T::Currency::settle(
                    &storage_provider,
                    imbalance,
                    WithdrawReasons::RESERVE,
                    KeepAlive,
                ) else {
                    log::error!(target: LOG_TARGET, "failed to settle currency after slashing... amount: {:?}, storage_provider: {:?}", slash_amount, storage_provider);
                    continue;
                };

                StorageProviders::<T>::insert(&storage_provider, state);
            }
        }

        /// Checks whether pre-committed sectors are expired and calculates slash amount.
        ///
        /// THIS FUNCTION DOES NOT HANDLE ERRORS!
        /// Code in hooks is assumed infallible and operates under invariants.
        ///
        /// Returns an array of expired sector numbers and the total deposit to be slashed.
        fn detect_expired_precommit_sectors(
            curr_block: BlockNumberFor<T>,
            state: &StorageProviderState<T::PeerId, BalanceOf<T>, BlockNumberFor<T>>,
        ) -> (
            BoundedVec<SectorNumber, ConstU32<MAX_SECTORS>>,
            BalanceOf<T>,
        ) {
            let mut expired_sectors: BoundedVec<SectorNumber, ConstU32<MAX_SECTORS>> =
                BoundedVec::new();
            let mut to_be_slashed = BalanceOf::<T>::zero();

            for (sector_number, sector) in &state.pre_committed_sectors {
                // Expiration marks the time for a block when it was supposed to be proven by `prove_commit` ultimately.
                // If it's still in `pre_commited_sectors` and `curr_block` is past this time, it means it was not.
                if curr_block >= sector.info.expiration {
                    let Ok(()) = expired_sectors.try_push(*sector_number) else {
                        log::error!(target: LOG_TARGET, "detect_expired_precommit_sectors: invariant violated, expired_sectors bounded_vec's capacity < state.pre_committed_sectors capacity, sector: {}", sector_number);
                        continue;
                    };
                    let Some(result) = to_be_slashed.checked_add(&sector.pre_commit_deposit) else {
                        log::error!(target: LOG_TARGET, "detect_expired_precommit_sectors: invariant violated, overflow in adding slash deposit: sector: {}, current: {:?}, to add: {:?}", sector_number, to_be_slashed, sector.pre_commit_deposit);
                        continue;
                    };
                    to_be_slashed = result;
                }
            }

            (expired_sectors, to_be_slashed)
        }
    }

    // Adapted from filecoin reference here: https://github.com/filecoin-project/builtin-actors/blob/54236ae89880bf4aa89b0dba6d9060c3fd2aacee/actors/miner/src/commd.rs#L51-L56
    fn validate_cid<T: Config>(bytes: &[u8]) -> Result<cid::Cid, Error<T>> {
        let c = Cid::try_from(bytes).map_err(|e| {
            log::error!(target: LOG_TARGET, "failed to validate cid: {:?}", e);
            Error::<T>::InvalidCid
        })?;
        // these values should be consistent with the cid's created by the SP.
        // They could change in the future when we make a definitive decision on what hashing algorithm to use and such
        ensure!(
            c.version() == Version::V1
                && c.codec() == CID_CODEC // The codec should align with our CID_CODEC value.
                && c.hash().code() == BLAKE2B_MULTIHASH_CODE // The CID should be hashed using blake2b
                && c.hash().size() == 32,
            Error::<T>::InvalidCid
        );

        Ok(c)
    }

    /// Calculate the required pre commit deposit amount
    fn calculate_pre_commit_deposit<T: Config>() -> BalanceOf<T> {
        1u32.into() // TODO(@aidan46, #106, 2024-06-24): Set a logical value or calculation
    }

    fn validate_seal_proof(
        _seal_proof_type: &RegisteredSealProof,
        proofs: BoundedVec<u8, ConstU32<256>>,
    ) -> bool {
        proofs.len() != 0 // TODO(@aidan46, no-ref, 2024-06-24): Actually check proof
    }
}
