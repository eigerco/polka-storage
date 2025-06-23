use cid::Cid;
use codec::Encode;
use frame_support::{
    dispatch::DispatchResult,
    ensure, fail,
    pallet_prelude::{CheckedAdd, Zero},
};
use frame_system::{
    ensure_signed,
    pallet_prelude::{BlockNumberFor, OriginFor},
};
use primitives::{
    commitment::{CommD, CommR, Commitment},
    randomness::DomainSeparationTag,
    sector::SectorPreCommitInfo,
    MAX_DEALS_PER_SECTOR, MAX_SECTORS_PER_CALL,
};
use sp_core::{ConstU32, Get};
use sp_runtime::BoundedVec;

use super::{calculate_pre_commit_deposit, get_randomness};
use crate::{
    dispatchables::verify_deals_for_activation, lock_funds, sector::SectorPreCommitOnChainInfo,
    storage_provider::StorageProviderState, BalanceOf, Config, Error, Event, Pallet,
    StorageProviders, LOG_TARGET,
};

pub fn pre_commit_sectors<T>(
    origin: OriginFor<T>,
    sectors: BoundedVec<SectorPreCommitInfo<BlockNumberFor<T>>, ConstU32<MAX_SECTORS_PER_CALL>>,
) -> DispatchResult
where
    T: Config,
{
    let owner = ensure_signed(origin)?;
    let sp =
        StorageProviders::<T>::try_get(&owner).map_err(|_| Error::<T>::StorageProviderNotFound)?;
    let current_block = <frame_system::Pallet<T>>::block_number();
    let sector_amount = sectors.len();

    // Pre-committed sectors for emitting the event.
    let mut pre_committed_sectors = BoundedVec::new();
    // All sectors in the batch to avoid mutating the SP multiple times.
    let mut on_chain_sectors: BoundedVec<
        SectorPreCommitOnChainInfo<BalanceOf<T>, BlockNumberFor<T>>,
        ConstU32<MAX_SECTORS_PER_CALL>,
    > = BoundedVec::new();
    // Total deposit amount to avoid mutating the SP multiple times and reserve only once.
    let mut total_deposit = BalanceOf::<T>::zero();
    // sector deals for all pre commits
    let mut all_sector_deals = BoundedVec::new();
    // unsealed_cids for all sectors
    let mut unsealed_cids = BoundedVec::new();
    // deal amounts for each sector
    let mut deal_amounts = BoundedVec::new();

    for sector in sectors {
        // Basic pre-commit validation.
        validate_sector_for_pre_commit::<T>(&sp, &sector)?;

        // Check that the expiration set by the SP makes sense.
        validate_expiration::<T>(
            current_block,
            current_block + T::MaxProveCommitDuration::get(),
            sector.expiration,
        )?;

        // Validate the data commitment
        let commd =
            Commitment::<CommD>::from_cid_bytes(&sector.unsealed_cid[..]).map_err(|err| {
                log::error!(target: LOG_TARGET, err:?; "pre_commit_sectors: invalid unsealed_cid");
                Error::<T>::InvalidCid
            })?;

        // Validate the replica commitment
        let _ = Commitment::<CommR>::from_cid_bytes(&sector.sealed_cid[..]).map_err(|err| {
            log::error!(target: LOG_TARGET, err:?; "pre_commit_sectors: invalid sealed_cid");
            Error::<T>::InvalidCid
        })?;

        let deposit = calculate_pre_commit_deposit::<T>();

        let entropy = owner.encode();
        let randomness = get_randomness::<T>(
            DomainSeparationTag::SealRandomness,
            sector.seal_randomness_height,
            &entropy,
        )?;

        let sector_on_chain =
            SectorPreCommitOnChainInfo::new(sector.clone(), deposit, current_block, randomness);

        // Push deal amounts for later verification
        deal_amounts.try_push(sector_on_chain.info.deal_ids.len()).expect("Programmer error: cannot have more that MAX_SECTORS_PER_CALL deal_amount because of previous bounds");
        // Push all unsealed_cids and deal amount to verify later.
        unsealed_cids.try_push(commd.cid()).expect("Programmer error: cannot have more that MAX_SECTORS_PER_CALL unsealed_cids because of previous bounds");
        // Push all deals to verify in one go later.
        all_sector_deals.try_push((&sector_on_chain).into()).expect(
                    "Programmer error: sector deals cannot be more that MAX_SECTORS_PER_CALL because of previous bounds",
                );
        // Add deposit to total deposit and push sector_on_chain to on_chain_sectors
        // to avoid mutation of the SP for every sector.
        total_deposit = total_deposit
                    .checked_add(&deposit)
                    .expect("Programmer error: Total deposit overflow should not happen because MAX_SECTORS_PER_CALL bound is lower than Balance::MAX");
        on_chain_sectors
                    .try_push(sector_on_chain)
                    .expect("Programmer error: on chain sectors should fit in this BoundedVec due to previous validation");
        // Push sector to BoundedVec for deposit event at the end
        pre_committed_sectors.try_push(sector).expect(
            "Programmer error: sectors should fit in this BoundedVec due to previous validation",
        );
    }

    let calculated_unsealed_cids = verify_deals_for_activation::<T>(&owner, all_sector_deals)?;

    check_commd_for_pre_commit::<T>(
        calculated_unsealed_cids,
        sector_amount,
        unsealed_cids,
        deal_amounts,
    )?;

    // Lock the pre-commit funds in the market account
    lock_funds::<T>(&owner, total_deposit)?;

    StorageProviders::<T>::try_mutate(&owner, |maybe_sp| -> DispatchResult {
        let sp = maybe_sp
            .as_mut()
            .ok_or(Error::<T>::StorageProviderNotFound)?;

        // NOTE(@jmg-duarte,21/1/25): Not sure if this is still needed
        sp.add_pre_commit_deposit(total_deposit)?;

        for sector_on_chain in on_chain_sectors {
            sp.put_pre_committed_sector(sector_on_chain)
                .map_err(|e| Error::<T>::GeneralPalletError(e))?;
        }
        Ok(())
    })?;

    Pallet::<T>::deposit_event(Event::SectorsPreCommitted {
        block: current_block,
        owner,
        sectors: pre_committed_sectors,
    });

    Ok(())
}

/// Checks if the sectors submitted for pre-commit by the SP are valid.
/// Checks are
/// - Sector number limit (cannot be higher than MAX_SECTORS)
/// - The proof type must correspond to the proof type submitted during registration.
/// - The sector number must not be used previously.
fn validate_sector_for_pre_commit<T>(
    sp: &StorageProviderState<T::PeerId, BalanceOf<T>, BlockNumberFor<T>>,
    sector: &SectorPreCommitInfo<BlockNumberFor<T>>,
) -> Result<(), Error<T>>
where
    T: Config,
{
    let sector_number = sector.sector_number;

    ensure!(
        sp.info.window_post_proof_type == sector.seal_proof.registered_window_post_proof(),
        Error::<T>::InvalidProofType
    );
    ensure!(
        !sp.pre_committed_sectors.contains_key(&sector_number)
            && !sp.sectors.contains_key(&sector_number),
        Error::<T>::SectorNumberAlreadyUsed
    );
    Ok(())
}

fn validate_expiration<T>(
    curr_block: BlockNumberFor<T>,
    activation: BlockNumberFor<T>,
    expiration: BlockNumberFor<T>,
) -> Result<(), Error<T>>
where
    T: Config,
{
    log::debug!(target: LOG_TARGET, "validate_expiration: {:?} {:?} {:?}", curr_block, activation, expiration);
    // Expiration must be after activation. Check this explicitly to avoid an underflow below.
    ensure!(
        expiration >= activation,
        Error::<T>::ExpirationBeforeActivation
    );
    // expiration cannot be less than minimum after activation
    ensure!(
        expiration - activation >= T::MinSectorExpiration::get(),
        Error::<T>::ExpirationTooSoon
    );
    // expiration cannot exceed MaxSectorExpiration from now
    ensure!(
        expiration < curr_block + T::MaxSectorExpiration::get(),
        Error::<T>::ExpirationTooLong,
    );
    // total sector lifetime cannot exceed SectorMaximumLifetime for the sector's seal proof
    ensure!(
        expiration - activation < T::SectorMaximumLifetime::get(),
        Error::<T>::MaxSectorLifetimeExceeded
    );
    Ok(())
}

/// Verifies that the unsealed_cid (CommD) and checks that it matches the given unsealed CID.
fn check_commd_for_pre_commit<T>(
    calculated_unsealed_cid: BoundedVec<Option<Cid>, ConstU32<MAX_DEALS_PER_SECTOR>>,
    sector_amount: usize,
    unsealed_cids: BoundedVec<Cid, ConstU32<MAX_DEALS_PER_SECTOR>>,
    deal_amounts: BoundedVec<usize, ConstU32<MAX_DEALS_PER_SECTOR>>,
) -> Result<(), Error<T>>
where
    T: Config,
{
    ensure!(calculated_unsealed_cid.len() == sector_amount, {
        log::error!(target: LOG_TARGET, "check_commd_for_pre_commit: failed to verify deals, invalid calculated_commd length: {}", calculated_unsealed_cid.len());
        Error::<T>::CouldNotVerifySectorForPreCommit
    });

    for (i, unsealed_cid) in unsealed_cids.into_iter().enumerate() {
        if deal_amounts[i] > 0 {
            let Some(calculated_commd) = calculated_unsealed_cid[i] else {
                log::error!(target: LOG_TARGET, "check_commd_for_pre_commit: commd for the deals at index {i} from verify_deals is None...");
                fail!(Error::<T>::CouldNotVerifySectorForPreCommit)
            };

            ensure!(calculated_commd == unsealed_cid, {
                log::error!(target: LOG_TARGET, "check_commd_for_pre_commit: calculated_commd at index {i} != sector.unsealed_cid, {:?} != {:?}", calculated_commd, unsealed_cid);
                Error::<T>::InvalidUnsealedCidForSector
            });
        }
    }
    Ok(())
}
