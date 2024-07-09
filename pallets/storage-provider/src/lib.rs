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
mod mock;

#[cfg(test)]
mod test;

mod proofs;
mod sector;
mod storage_provider;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    pub const CID_CODEC: u64 = 0x55;
    /// Sourced from multihash code table <https://github.com/multiformats/rust-multihash/blob/b321afc11e874c08735671ebda4d8e7fcc38744c/codetable/src/lib.rs#L108>
    pub const BLAKE2B_MULTIHASH_CODE: u64 = 0xB220;
    pub const LOG_TARGET: &'static str = "runtime::storage_provider";

    use core::fmt::Debug;

    use cid::{Cid, Version};
    use codec::{Decode, Encode};
    use frame_support::{
        dispatch::DispatchResult,
        ensure,
        pallet_prelude::*,
        traits::{Currency, ReservableCurrency},
    };
    use frame_system::{ensure_signed, pallet_prelude::*, Config as SystemConfig};
    use primitives_proofs::{Market, RegisteredPoStProof, RegisteredSealProof, SectorNumber};
    use scale_info::TypeInfo;

    use crate::{
        proofs::{
            assign_proving_period_offset, current_deadline_index, current_proving_period_start,
        },
        sector::{
            ProveCommitSector, SectorOnChainInfo, SectorPreCommitInfo, SectorPreCommitOnChainInfo,
            SECTORS_MAX,
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
        /// Proving period for submitting Window PoSt, 24 hours is blocks
        #[pallet::constant]
        type WPoStProvingPeriod: Get<BlockNumberFor<Self>>;
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
        /// Emitted when a storage provider tries to commit more sectors than MAX_SECTORS.
        MaxPreCommittedSectorExceeded,
        /// Emitted when a sector fails to activate.
        SectorActivateFailed,
        /// Emitted when removing a precommitted sector after proving fails.
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
        /// Emitted when a prove commit is sent after the dealine
        /// These precommits will be cleaned up in the hook
        ProveCommitAfterDeadline,
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
            let state = StorageProviderState::new(&info, period_start, deadline_idx);
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
                sector_number <= SECTORS_MAX.into(),
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
            validate_cid::<T>(&sector.unsealed_cid[..])?;
            let balance = T::Currency::total_balance(&owner);
            let deposit = calculate_pre_commit_deposit::<T>();
            Self::validate_expiration(
                current_block,
                current_block + T::MaxProveCommitDuration::get(),
                sector.expiration,
            )?;
            ensure!(balance >= deposit, Error::<T>::NotEnoughFunds);
            T::Currency::reserve(&owner, deposit)?;
            StorageProviders::<T>::try_mutate(&owner, |maybe_sp| -> DispatchResult {
                let sp = maybe_sp
                    .as_mut()
                    .ok_or(Error::<T>::StorageProviderNotFound)?;
                sp.add_pre_commit_deposit(deposit)?;
                sp.put_precommitted_sector(SectorPreCommitOnChainInfo::new(
                    sector.clone(),
                    deposit,
                    <frame_system::Pallet<T>>::block_number(),
                ))
                .map_err(|_| Error::<T>::MaxPreCommittedSectorExceeded)?;
                Ok(())
            })?;
            Self::deposit_event(Event::SectorPreCommitted { owner, sector });
            Ok(())
        }

        /// Allows the SP to submit proof for their precomitted sectors.
        /// TODO(@aidan46, no-ref, 2024-06-24): Add functionality to allow for batch pre commit
        pub fn prove_commit_sector(
            origin: OriginFor<T>,
            sector: ProveCommitSector,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            let sp = StorageProviders::<T>::try_get(&owner)
                .map_err(|_| Error::<T>::StorageProviderNotFound)?;
            let sector_number = sector.sector_number;
            ensure!(
                sector_number <= SECTORS_MAX.into(),
                Error::<T>::InvalidSector
            );
            let precommit = sp
                .get_precommitted_sector(sector_number)
                .map_err(|_| Error::<T>::InvalidSector)?;
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
                sp.activate_sector(sector_number, new_sector)
                    .map_err(|_| Error::<T>::SectorActivateFailed)?;
                sp.remove_precomitted_sector(sector_number)
                    .map_err(|_| Error::<T>::CouldNotRemoveSector)?;
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
    }

    // Adapted from filecoin reference here: https://github.com/filecoin-project/builtin-actors/blob/54236ae89880bf4aa89b0dba6d9060c3fd2aacee/actors/miner/src/commd.rs#L51-L56
    fn validate_cid<T: Config>(bytes: &[u8]) -> Result<(), Error<T>> {
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
        Ok(())
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
