mod activate_deals;
mod add_balance;
mod on_sectors_terminate;
mod publish_deal_parameters;
mod publish_storage_deals;
mod remove_deal_parameters;
mod settle_deal_payments;
mod verify_deals_for_activation;
mod withdraw_balance;

pub use activate_deals::activate_deals;
pub use add_balance::add_balance;
pub use on_sectors_terminate::on_sectors_terminate;
pub use publish_deal_parameters::publish_deal_parameters;
pub use publish_storage_deals::publish_storage_deals;
pub use remove_deal_parameters::remove_deal_parameters;
pub use settle_deal_payments::settle_deal_payments;
pub use verify_deals_for_activation::verify_deals_for_activation;
pub use withdraw_balance::withdraw_balance;

extern crate alloc;

use alloc::vec::Vec;

use cid::Cid;
use frame_support::{dispatch::DispatchResult, ensure, traits::ConstU32};
use frame_system::pallet_prelude::BlockNumberFor;
use primitives::{
    commitment::{
        commd::compute_unsealed_sector_commitment,
        piece::{PaddedPieceSize, PieceInfo},
    },
    configs::BalanceOf,
    deals::{DealProposalOf, DealState},
    proofs::RegisteredSealProof,
    sector::{SectorNumber, SectorSize},
    DealId, MAX_DEALS_FOR_ALL_SECTORS, MAX_DEALS_PER_SECTOR,
};
use sp_runtime::{
    traits::{CheckedAdd, CheckedSub},
    ArithmeticError, BoundedBTreeSet, BoundedVec, DispatchError,
};

use crate::{
    error::CommDError, BalanceTable, Config, Error, Pallet, PendingProposals, Proposals, LOG_TARGET,
};

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

fn proposals_for_deals<T>(
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
