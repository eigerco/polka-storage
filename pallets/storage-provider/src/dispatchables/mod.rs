mod activate_deals;
mod add_balance;
mod declare_faults;
mod declare_faults_recovered;
mod on_sectors_terminate;
mod pre_commit_sectors;
mod prove_commit_sectors;
mod publish_deal_parameters;
mod publish_storage_deals;
mod register_storage_provider;
mod remove_deal_parameters;
mod settle_deal_payments;
mod submit_windowed_post;
mod terminate_sectors;
mod verify_deals_for_activation;
mod withdraw_balance;

pub use activate_deals::activate_deals;
pub use add_balance::add_balance;
pub use declare_faults::declare_faults;
pub use declare_faults_recovered::declare_faults_recovered;
pub use on_sectors_terminate::on_sectors_terminate;
pub use pre_commit_sectors::pre_commit_sectors;
pub use prove_commit_sectors::prove_commit_sectors;
pub use publish_deal_parameters::publish_deal_parameters;
pub use publish_storage_deals::publish_storage_deals;
pub use register_storage_provider::register_storage_provider;
pub use remove_deal_parameters::remove_deal_parameters;
pub use settle_deal_payments::settle_deal_payments;
pub use submit_windowed_post::submit_windowed_post;
pub use terminate_sectors::terminate_sectors;
pub use verify_deals_for_activation::verify_deals_for_activation;
pub use withdraw_balance::withdraw_balance;

extern crate alloc;

use alloc::vec::Vec;

use cid::Cid;
use frame_support::{
    dispatch::DispatchResult,
    ensure,
    pallet_prelude::{DispatchError, One},
    traits::ConstU32,
};
use frame_system::pallet_prelude::BlockNumberFor;
use primitives::{
    commitment::{
        commd::compute_unsealed_sector_commitment,
        piece::{PaddedPieceSize, PieceInfo},
    },
    deals::DealState,
    proofs::RegisteredSealProof,
    randomness::{draw_randomness, AuthorVrfHistory, DomainSeparationTag},
    sector::{SectorNumber, SectorSize},
    DealId, MAX_DEALS_FOR_ALL_SECTORS, MAX_DEALS_PER_SECTOR,
};
use sp_core::Get;
use sp_runtime::{
    traits::{CheckedAdd, CheckedSub},
    ArithmeticError, BoundedBTreeSet, BoundedVec,
};

use crate::{
    error::CommDError, BalanceOf, BalanceTable, Config, DealProposalOf, Error, Pallet,
    PendingProposals, Proposals, StorageProviders, LOG_TARGET,
};

/// Calculate the required pre commit deposit amount
pub(crate) fn calculate_pre_commit_deposit<T>() -> BalanceOf<T>
where
    T: Config,
{
    BalanceOf::<T>::one() // TODO(@aidan46, #106, 2024-06-24): Set a logical value or calculation
}

/// Processes terminations for the given account (should be a registered SP).
/// Clears all early terminations and calls `on_sectors_terminate` when finished.
pub(crate) fn process_early_terminations<T>(
    current_block: BlockNumberFor<T>,
    owner: &T::AccountId,
) -> Result<(), Error<T>>
where
    T: Config,
{
    let mut state =
        StorageProviders::<T>::try_get(owner).map_err(|_| Error::<T>::StorageProviderNotFound)?;
    let result = state
        .pop_early_terminations(
            T::AddressedPartitionsMax::get(),
            T::AddressedSectorsMax::get(),
        )
        .map_err(|e| Error::<T>::GeneralPalletError(e))?;

    // Nothing to do, don't waste any time.
    // This can happen if we end up processing early terminations
    // before the cron callback fires.
    if result.is_empty() {
        log::info!(target: LOG_TARGET, "no early terminations");
        return Ok(());
    }

    let mut sectors_with_data = Vec::new();
    // Check whether sectors have expired, if not push to sectors_with_data for later processing.
    // Process in Market pallet `on_sectors_terminate`.
    // TODO(@aidan46, #452, 2024-10-14): Figure out economics to apply early termination penalty.
    for (&expiry, sector_numbers) in result.sectors.iter() {
        for sector_number in sector_numbers {
            // I am not 100% sure this is correct. In FC they use deal weight to determine.
            // Deal weight is a function of space times the duration of a deal.
            if expiry < current_block {
                sectors_with_data.push(*sector_number);
            }
        }
    }

    // Terminate deals
    let terminated_data = sectors_with_data.try_into().expect(
                "The sectors in the result can never be more than MAX_DEALS_PER_SECTOR due to previous bounds, this should not fail",
            );

    on_sectors_terminate::<T>(owner, terminated_data)
        .map_err(|_| Error::<T>::CouldNotTerminateDeals)?;

    // Update storage provider state
    StorageProviders::<T>::insert(owner, state);

    Ok(())
}

/// Get randomness from the chain and process it with domain separation.
pub(crate) fn get_randomness<T: Config>(
    personalization: DomainSeparationTag,
    block_number: BlockNumberFor<T>,
    entropy: &[u8],
) -> Result<[u8; 32], DispatchError> {
    // Get randomness from chain
    let Some(randomness) = T::AuthorVrfHistory::author_vrf_history(block_number) else {
        return Err(Error::<T>::MissingAuthorVRF.into());
    };

    // Converting block_height to the type accepted by draw_randomness
    let block_number = block_number.try_into().map_err(|_| {
        log::error!(target: LOG_TARGET, "get_randomness: failed to convert block_height to u64");
        Error::<T>::ConversionError
    })?;

    // HACK: convert an unsized slice to a sized one
    // SAFETY: we know that the output is 32 bytes since we control the chain config
    let mut sized_randomness = [0; 32];
    sized_randomness.copy_from_slice(randomness.as_ref());

    // Randomness with the bias
    let randomness = draw_randomness(&sized_randomness, personalization, block_number, entropy);

    Ok(randomness)
}

/// Moves the provided `amount` from the `client`'s locked funds, to the provider's `free` funds.
///
/// # Pre-Conditions
/// * The client MUST have the necessary funds locked.
pub fn perform_storage_payment<T>(
    client: &T::AccountId,
    provider: &T::AccountId,
    amount: BalanceOf<T>,
) -> DispatchResult
where
    T: Config,
{
    // These should have been checked when locking funds
    BalanceTable::<T>::try_mutate(client, |balance| -> DispatchResult {
        let locked = balance
            .locked
            .checked_sub(&amount)
            .ok_or(ArithmeticError::Underflow)?;
        balance.locked = locked;
        Ok(())
    })?;

    BalanceTable::<T>::try_mutate(provider, |balance| -> DispatchResult {
        let free = balance
            .free
            .checked_add(&amount)
            .ok_or(ArithmeticError::Overflow)?;
        balance.free = free;
        Ok(())
    })?;

    Ok(())
}

pub fn proposals_for_deals<T>(
    deal_ids: BoundedVec<DealId, ConstU32<MAX_DEALS_PER_SECTOR>>,
) -> Result<
    BoundedVec<(DealId, DealProposalOf<T>), ConstU32<MAX_DEALS_FOR_ALL_SECTORS>>,
    DispatchError,
>
where
    T: Config,
{
    let mut unique_deals: BoundedBTreeSet<DealId, ConstU32<MAX_DEALS_PER_SECTOR>> =
        BoundedBTreeSet::new();
    let mut proposals = BoundedVec::new();
    for deal_id in deal_ids {
        ensure!(!unique_deals.contains(&deal_id), {
            log::error!(target: LOG_TARGET, "deal {} is duplicated", deal_id);
            Error::<T>::DuplicateDeal
        });

        // PRE-COND: always succeeds, unique_deals has the same boundary as sector.deal_ids[]
        unique_deals.try_insert(deal_id).map_err(|deal_id| {
            log::error!(target: LOG_TARGET, "failed to insert deal {}", deal_id);
            Error::<T>::DealPreconditionFailed
        })?;

        let proposal: DealProposalOf<T> = Proposals::<T>::try_get(&deal_id).map_err(|_| {
            log::error!(target: LOG_TARGET, "deal {} not found", deal_id);
            Error::<T>::DealNotFound
        })?;

        // PRE-COND: always succeeds, unique_deals has the same boundary as sector.deal_ids[]
        proposals.try_push((deal_id, proposal)).map_err(|_| {
            log::error!(target: LOG_TARGET, "failed to insert deal {} into proposals", deal_id);
            Error::<T>::DealPreconditionFailed
        })?;
    }

    Ok(proposals)
}

/// <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/lib.rs#L1388>
fn validate_deals_for_sector<T>(
    deals: &BoundedVec<(DealId, DealProposalOf<T>), ConstU32<MAX_DEALS_FOR_ALL_SECTORS>>,
    provider: &T::AccountId,
    sector_number: SectorNumber,
    sector_expiry: BlockNumberFor<T>,
    sector_activation: BlockNumberFor<T>,
    sector_size: SectorSize,
) -> DispatchResult
where
    T: Config,
{
    let mut total_deal_space = 0;
    for (deal_id, deal) in deals {
        validate_deal_can_activate::<T>(deal, provider, sector_expiry, sector_activation)
            .map_err(|e| {
                log::error!(target: LOG_TARGET, "deal {} cannot be activated, because: {:?}", *deal_id, e);
                e
            })?;
        total_deal_space += deal.piece_size;
    }

    ensure!(total_deal_space <= sector_size.bytes(), {
        log::error!(target: LOG_TARGET, "cannot fit all of the deals into sector {}, {} < {}", sector_number, total_deal_space, sector_size.bytes());
        Error::<T>::DealsTooLargeToFitIntoSector
    });

    Ok(())
}

/// <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/lib.rs#L1570>
fn validate_deal_can_activate<T>(
    deal: &DealProposalOf<T>,
    provider: &T::AccountId,
    sector_expiry: BlockNumberFor<T>,
    sector_activation: BlockNumberFor<T>,
) -> Result<(), Error<T>>
where
    T: Config,
{
    ensure!(*provider == deal.provider, Error::<T>::InvalidProvider);
    ensure!(
        deal.state == DealState::Published,
        Error::<T>::InvalidDealState
    );
    ensure!(
        sector_activation <= deal.start_block,
        Error::<T>::StartBlockElapsed
    );
    ensure!(
        sector_expiry >= deal.end_block,
        Error::<T>::SectorExpiresBeforeDeal
    );

    // Confirm the deal is in the pending proposals set.
    // It will be removed from this queue later, during cron.
    // Failing this check is an internal invariant violation.
    // The pending deals set exists to prevent duplicate proposals.
    // It should be impossible to have a proposal, no deal state, and not be in pending deals.
    let hash = Pallet::<T>::hash_proposal(&deal);
    ensure!(
        PendingProposals::<T>::get().contains(&hash),
        Error::<T>::DealNotPending
    );

    Ok(())
}

/// <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/lib.rs#L1370>
fn compute_commd<'a, T>(
    proposals: impl Iterator<Item = &'a DealProposalOf<T>>,
    sector_type: RegisteredSealProof,
) -> Result<Cid, DispatchError>
where
    T: Config,
{
    let pieces = proposals
        .map(|p| {
            let commitment = p.piece_commitment().map_err(|e| {
                log::error!(target: LOG_TARGET, "compute_commd: CommitmentError {e}");
                CommDError::CommitmentError(e)
            })?;
            let size = PaddedPieceSize::new(p.piece_size).map_err(|e| {
                log::error!(target: LOG_TARGET, "compute_commd: PaddedPieceSizeError {e:?}");
                CommDError::PaddedPieceSizeError(e)
            })?;

            Ok(PieceInfo { size, commitment })
        })
        .collect::<Result<Vec<_>, CommDError>>();

    let pieces = pieces.map_err(|err| {
        log::error!("error occurred while processing pieces: {:?}", err);
        Error::<T>::CommD
    })?;

    let sector_size = sector_type.sector_size();
    let comm_d = compute_unsealed_sector_commitment(sector_size, &pieces).map_err(|err| {
        log::error!("error occurred while computing commd: {:?}", err);
        Error::<T>::CommD
    })?;

    Ok(comm_d.cid())
}
