use frame_support::{
    dispatch::DispatchResult, pallet_prelude::*, sp_runtime::ArithmeticError, traits::ConstU32,
};
use frame_system::pallet_prelude::*;
use primitives::{deals::DealState, sector::SectorNumber, MAX_DEALS_PER_SECTOR};
use sp_arithmetic::traits::BaseArithmetic;

use crate::{
    dispatchables::perform_storage_payment, slash_and_burn, unlock_funds, BalanceOf, Config, Error,
    Event, Pallet, PendingProposals, Proposals, SectorDeals,
};

pub fn on_sectors_terminate<T>(
    storage_provider: &T::AccountId,
    sectors: BoundedVec<SectorNumber, ConstU32<MAX_DEALS_PER_SECTOR>>,
) -> DispatchResult
where
    T: Config,
{
    // TODO(@jmg-duarte,04/07/2024): check that the caller is actually a storage provider (?)

    // NOTE(@jmg-duarte,03/07/2024): the usage of the `current_block` NEEDS to be revised
    // in the future as this function MAY be called on a different block than the current one.
    // This is a consequence of the fact that this function is called indirectly,
    // through a chain of calls that start on deferred cron events
    let current_block = <frame_system::Pallet<T>>::block_number();

    for sector_id in sectors {
        // In the original implementation, all sectors are popped, here, we take them all
        let Some(deal_ids) = SectorDeals::<T>::take((storage_provider, sector_id)) else {
            // Not found sectors are ignored, if we don't find any, we don't do anything
            continue;
        };

        for deal_id in deal_ids {
            // Fetch the corresponding deal proposal, it's ok if it has already been deleted
            let Some(mut deal_proposal) = Proposals::<T>::get(deal_id) else {
                return Err(Error::<T>::DealNotFound.into());
            };

            // This should never happen, because we are getting deals
            // the storage provider with which we called the extrinsic.
            if *storage_provider != deal_proposal.provider {
                return Err(Error::<T>::InvalidCaller.into());
            }

            if deal_proposal.end_block <= current_block {
                // not slashing finished deals
                continue;
            }

            let hash_proposal = Pallet::<T>::hash_proposal(&deal_proposal);
            // If a sector is being terminated, it means that at some point,
            // the deals contained within were active
            let DealState::Active(ref mut active_deal_state) = deal_proposal.state else {
                return Err(Error::<T>::DealIsNotActive.into());
            };

            // https://github.com/filecoin-project/builtin-actors/blob/54236ae89880bf4aa89b0dba6d9060c3fd2aacee/actors/market/src/lib.rs#L840-L844
            if let Some(_) = active_deal_state.slash_block {
                log::warn!("deal {} was already slashed, terminating anyway", deal_id);
            }

            // https://github.com/filecoin-project/builtin-actors/blob/54236ae89880bf4aa89b0dba6d9060c3fd2aacee/actors/market/src/lib.rs#L846-L850
            if let None = active_deal_state.last_updated_block {
                PendingProposals::<T>::mutate(|pending_proposals| {
                    pending_proposals.remove(&hash_proposal);
                });
            }

            // Handle payments
            // https://github.com/filecoin-project/builtin-actors/blob/54236ae89880bf4aa89b0dba6d9060c3fd2aacee/actors/market/src/state.rs#L922-L962

            // https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/state.rs#L932-L933
            let payment_start_block = calculate_start_block(
                deal_proposal.start_block,
                active_deal_state.last_updated_block,
            );
            // The only reason we can use `current_block` is because of the line
            // https://github.com/filecoin-project/builtin-actors/blob/54236ae89880bf4aa89b0dba6d9060c3fd2aacee/actors/market/src/lib.rs#L852
            let payment_end_block = calculate_end_block(current_block, deal_proposal.end_block);
            let n_blocks_elapsed = calculate_elapsed_blocks(payment_start_block, payment_end_block);

            let total_payment = calculate_storage_price::<T>(
                n_blocks_elapsed,
                deal_proposal.storage_price_per_block,
            )?;

            // Pay any outstanding debts to the provider
            perform_storage_payment::<T>(
                &deal_proposal.client,
                &deal_proposal.provider,
                total_payment,
            )?;

            let provider_collateral: BalanceOf<T> = deal_proposal
                .provider_collateral()
                .ok_or(Error::<T>::UnexpectedValidationError)?
                .try_into()
                .map_err(|_| Error::<T>::UnexpectedValidationError)?;

            // Slash and burn the provider collateral
            slash_and_burn::<T>(&deal_proposal.provider, provider_collateral)?;

            // The remaining client locked funds should be counted from
            // everything we just paid until the deal's end block
            let remaining_client_collateral = calculate_storage_price::<T>(
                deal_proposal.end_block - payment_end_block,
                deal_proposal.storage_price_per_block,
            )?;
            // We then unlock those client funds
            unlock_funds::<T>(&deal_proposal.client, remaining_client_collateral)?;

            // Remove completed deal
            let _ = Proposals::<T>::remove(deal_id);

            Pallet::<T>::deposit_event(Event::<T>::DealTerminated {
                deal_id,
                client: deal_proposal.client.clone(),
                provider: deal_proposal.provider.clone(),
            });
        }
    }
    Ok(())
}

/// Calculate the start block.
///
/// If `last_updated_block` is `None`, returns `start_block`.
/// Otherwise, returns the `max` between `start_block` and `last_updated_block`.
#[inline(always)]
fn calculate_start_block<BlockNumber>(
    start_block: BlockNumber,
    last_updated_block: Option<BlockNumber>,
) -> BlockNumber
where
    BlockNumber: BaseArithmetic,
{
    if let Some(last_updated_block) = last_updated_block {
        core::cmp::max(start_block, last_updated_block)
    } else {
        start_block
    }
}

/// Calculate the end block.
///
/// Returns the `min` between the `current_block` and `end_block`.
#[inline(always)]
fn calculate_end_block<BlockNumber>(
    current_block: BlockNumber,
    end_block: BlockNumber,
) -> BlockNumber
where
    BlockNumber: BaseArithmetic,
{
    core::cmp::min(current_block, end_block)
}

/// Calculate the number of elapsed blocks.
///
/// Returns the `max` between `end_block - start_block` and `0`.
#[inline(always)]
fn calculate_elapsed_blocks<BlockNumber>(
    start_block: BlockNumber,
    end_block: BlockNumber,
) -> BlockNumber
where
    BlockNumber: BaseArithmetic,
{
    // https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/state.rs#L934-L935
    core::cmp::max(end_block - start_block, 0.into())
}

/// Calculate the storage price for a given `n_blocks` at a rate of `price_per_block`.
///
/// Internally, this function converts both values to [`u128`], multiplies them,
/// and converts back to [`BalanceOf<T>`], if at any point the conversion fails,
/// it is assumed to be an overflow and [`ArithmeticError::Overflow`] is returned.
#[inline(always)]
fn calculate_storage_price<T>(
    n_blocks: BlockNumberFor<T>,
    price_per_block: BalanceOf<T>,
) -> Result<BalanceOf<T>, ArithmeticError>
where
    T: Config,
{
    let n_blocks = TryInto::<u128>::try_into(n_blocks).map_err(|_| ArithmeticError::Overflow)?;
    let price_per_block =
        TryInto::<u128>::try_into(price_per_block).map_err(|_| ArithmeticError::Overflow)?;
    TryInto::try_into(price_per_block * n_blocks).map_err(|_| ArithmeticError::Overflow)
}
