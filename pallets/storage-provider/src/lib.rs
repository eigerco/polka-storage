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
    pub const LOG_TARGET: &'static str = "runtime::storage_provider";

    extern crate alloc;

    use alloc::vec;
    use core::fmt::Debug;

    use cid::{Cid, Version};
    use codec::{Decode, Encode};
    use frame_support::{
        dispatch::DispatchResult,
        ensure, fail,
        pallet_prelude::*,
        traits::{Currency, ReservableCurrency},
    };
    use frame_system::{ensure_signed, pallet_prelude::*, Config as SystemConfig};
    use primitives_proofs::{Market, RegisteredPoStProof, RegisteredSealProof, SectorNumber};
    use scale_info::TypeInfo;

    use crate::{
        deadline::DeadlineInfo,
        fault::DeclareFaultsParams,
        proofs::{
            assign_proving_period_offset, current_deadline_index, current_proving_period_start,
            SubmitWindowedPoStParams,
        },
        sector::{
            ProveCommitSector, SectorOnChainInfo, SectorPreCommitInfo, SectorPreCommitOnChainInfo,
            MAX_SECTORS,
        },
        storage_provider::{StorageProviderInfo, StorageProviderState},
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
        /// Window PoSt challenge window (default 30 minutes in blocks)
        #[pallet::constant]
        type WPoStChallengeWindow: Get<BlockNumberFor<Self>>;

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

        /// Maximum number of unique "declarations" in batch operations.
        #[pallet::constant]
        type DeclarationsMax: Get<u64>;
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
        /// Emitted when an SP submits a valid PoSt
        ValidPoStSubmitted { owner: T::AccountId },
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
        /// Emitted when an SP tries to declare too many faults in 1 extrinsic (max is DeclartionsMax).
        TooManyDeclartions,
        /// Wrapper around the [`DeadlineError`] type.
        DeadlineError(crate::deadline::DeadlineError),
        /// Wrapper around the [`PartitionError`] type.
        PartitionError(crate::partition::PartitionError),
        /// Wrapper around the [`StorageProviderError`] type.
        StorageProviderError(crate::storage_provider::StorageProviderError),
        /// Emitted when Market::verify_deals_for_activation fails for an unexpected reason.
        /// Verification happens in pre_commit, to make sure a sector is precommited with valid deals.
        CouldNotVerifySectorForPreCommit,
        /// Declared unsealed_cid for pre_commit is different from the one calcualated by `Market::verify_deals_for_activation`.
        /// unsealed_cid === CommD and is calculated from piece ids of all of the deals in a sector.
        InvalidUnsealedCidForSector,
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
            let proving_period = T::WPoStProvingPeriod::get();
            let current_block = <frame_system::Pallet<T>>::block_number();
            let offset = assign_proving_period_offset::<T::AccountId, BlockNumberFor<T>>(
                &owner,
                current_block,
                proving_period,
            )
            .map_err(|_| Error::<T>::ConversionError)?;
            let period_start = current_proving_period_start(current_block, offset, proving_period);
            let deadline_idx =
                current_deadline_index(current_block, period_start, T::WPoStChallengeWindow::get());
            let info = StorageProviderInfo::new(peer_id, window_post_proof_type);
            let state = StorageProviderState::new(
                &info,
                period_start,
                deadline_idx,
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
                    T::WPoStChallengeWindow::get(),
                    T::WPoStPeriodDeadlines::get(),
                    T::WPoStProvingPeriod::get(),
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
            let mut sp = StorageProviders::<T>::try_get(&owner)
                .map_err(|_| Error::<T>::StorageProviderNotFound)?;
            if let Err(e) = Self::validate_windowed_post(
                current_block,
                &windowed_post,
                sp.info.window_post_proof_type,
            ) {
                log::error!(target: LOG_TARGET, "submit_window_post: PoSt submission is invalid {e:?}");
                return Err(e.into());
            }
            let current_deadline = sp
                .deadline_info(
                    current_block,
                    T::WPoStChallengeWindow::get(),
                    T::WPoStPeriodDeadlines::get(),
                    T::WPoStProvingPeriod::get(),
                )
                .map_err(|e| Error::<T>::DeadlineError(e))?;
            Self::validate_deadline(current_block, &current_deadline, &windowed_post)?;
            let deadlines = sp.get_deadlines_mut();
            log::debug!(target: LOG_TARGET, "submit_windowed_post: deadlines = {deadlines:#?}");
            // record sector as proven
            deadlines
                .record_proven(windowed_post.deadline as usize, windowed_post.partition)
                .map_err(|e| Error::<T>::DeadlineError(e))?;
            log::debug!(target: LOG_TARGET, "submit_windowed_post: proof recorded");
            Self::deposit_event(Event::ValidPoStSubmitted { owner });
            Ok(())
        }

        /// The SP uses this extrinsic to declare some sectors as faulty. Letting the system know it will not submit PoSt for the next deadline.
        pub fn declare_faults(origin: OriginFor<T>, params: DeclareFaultsParams) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            ensure!(
                params.faults.len() as u64 <= T::DeclarationsMax::get(),
                Error::<T>::TooManyDeclartions
            );
            let mut _sp = StorageProviders::<T>::try_get(&owner)
                .map_err(|_| Error::<T>::StorageProviderNotFound)?;
            Ok(())
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
        /// - The block the deadline was committed at is after the current block.
        fn validate_deadline(
            curr_block: BlockNumberFor<T>,
            current_deadline: &DeadlineInfo<BlockNumberFor<T>>,
            post_params: &SubmitWindowedPoStParams<BlockNumberFor<T>>,
        ) -> Result<(), Error<T>> {
            ensure!(current_deadline.is_open(), {
                log::error!(target: LOG_TARGET, "validate_deadline: {current_deadline:?}, deadline isn't open");
                Error::<T>::InvalidDeadlineSubmission
            });
            ensure!(post_params.deadline == current_deadline.idx, {
                log::error!(target: LOG_TARGET, "validate_deadline: given index does not match current index {} != {}", post_params.deadline, current_deadline.idx);
                Error::<T>::InvalidDeadlineSubmission
            });
            ensure!(post_params.chain_commit_block < curr_block, {
                log::error!(target: LOG_TARGET, "validate_deadline: chain commit block is after current block {:?} > {curr_block:?}", post_params.chain_commit_block);
                Error::<T>::InvalidDeadlineSubmission
            });
            Ok(())
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
