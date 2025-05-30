use frame_support::{dispatch::DispatchResult, pallet_prelude::*, sp_runtime::ArithmeticError};
use frame_system::pallet_prelude::*;
use primitives::{configs::BalanceOf, deals::DealState, DealId};

use super::perform_storage_payment;
use crate::{
    error::DealSettlementError, unlock_funds, Config, Event, MaxSettleDeals, Pallet, Proposals,
    SettledDealData, LOG_TARGET,
};

pub fn settle_deal_payments<T>(
    origin: OriginFor<T>,
    // The original `deals` structure is a bitfield from fvm-ipld-bitfield
    deal_ids: BoundedVec<DealId, MaxSettleDeals<T>>,
) -> DispatchResult
where
    T: Config,
{
    // Anyone with gas can settle payments, so we just check if the origin is signed
    ensure_signed(origin)?;

    // INVARIANT: slashed deals cannot show up here because slashing is fully processed by `on_sector_terminate`

    let current_block = <frame_system::Pallet<T>>::block_number();

    let mut successful = BoundedVec::<_, MaxSettleDeals<T>>::new();
    let mut unsuccessful = BoundedVec::<_, MaxSettleDeals<T>>::new();

    for deal_id in deal_ids {
        // If the deal is not found, we register an error and move on
        // https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/lib.rs#L1225-L1231
        let Some(mut deal_proposal) = Proposals::<T>::get(deal_id) else {
            log::error!(target: LOG_TARGET, "deal not found — deal_id: {}", deal_id);
            // SAFETY: Always succeeds because the upper bound on the vecs should be the same as the input vec
            let _ = unsuccessful.try_push((deal_id, DealSettlementError::DealNotFound));
            continue;
        };

        // Deal isn't possibly valid yet
        // https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/lib.rs#L1255-L1264
        if deal_proposal.start_block > current_block {
            // SAFETY: Always succeeds because the upper bound on the vecs should be the same as the input vec
            let _ = unsuccessful.try_push((deal_id, DealSettlementError::EarlySettlement));
            continue;
        }

        // If the deal is not active (i.e. unpublished or published), there's nothing to settle
        // https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/lib.rs#L1225-L1231
        let DealState::Active(ref mut active_deal_state) = deal_proposal.state else {
            // If a deal is not published, there's nothing to settle
            // If a deal is published, but not active, it's supposed to be removed by cron/hooks

            // NOTE(@jmg-duarte,28/06/2024): maybe we should handle deals where deal_proposal.start_block < current_block — i.e. expired

            // SAFETY: Always succeeds because the upper bound on the vecs should be the same as the input vec
            let _ = unsuccessful.try_push((deal_id, DealSettlementError::DealNotActive));
            continue;
        };

        // If the last updated block is in the future, return an error
        if let Some(last_updated_block) = active_deal_state.last_updated_block {
            if last_updated_block > current_block {
                log::error!(target: LOG_TARGET,
                    "last_updated_block for deal is in the future — deal_id: {}, last_updated_block: {:?}",
                    deal_id,
                    last_updated_block
                );
                // SAFETY: Always succeeds because the upper bound on the vecs should be the same as the input vec
                let _ = unsuccessful.try_push((deal_id, DealSettlementError::FutureLastUpdate));
                continue;
            }
        }

        // If we never settled, the duration starts at `start_block`
        let last_settled_block = active_deal_state
            .last_updated_block
            .unwrap_or(deal_proposal.start_block);

        if last_settled_block > deal_proposal.end_block {
            // If the code reaches this, it's a big whoops
            log::error!(target: LOG_TARGET, "the last settled block cannot be bigger than the end block — last_settled_block: {:?}, end_block: {:?}",
                        last_settled_block, deal_proposal.end_block);
            return Err(DispatchError::Corruption);
        }

        let (block_to_settle, complete_deal) = {
            if current_block >= deal_proposal.end_block {
                // The deal has been completed, as such, we'll remove it later on
                (deal_proposal.end_block, true)
            } else {
                (current_block, false)
            }
        };

        // If an error happens when converting here we have more to worry about than completing all settlements
        let deal_settlement_amount: BalanceOf<T> = {
            // There's no great way to avoid the repeated code without macros or more generics magic
            // ArithmeticError::Overflow used as `duration` and `storage_price_per_block` can only be positive
            let duration: u128 = (block_to_settle - last_settled_block)
                .try_into()
                .map_err(|_| DispatchError::Arithmetic(ArithmeticError::Overflow))?;
            let storage_price_per_block: u128 = deal_proposal
                .storage_price_per_block
                .try_into()
                .map_err(|_| DispatchError::Arithmetic(ArithmeticError::Overflow))?;

            (duration * storage_price_per_block)
                .try_into()
                .map_err(|_| DispatchError::Arithmetic(ArithmeticError::Overflow))
        }?;

        perform_storage_payment::<T>(
            &deal_proposal.client,
            &deal_proposal.provider,
            deal_settlement_amount,
        )?;

        // SAFETY: Always succeeds because the upper bound on the vecs should be the same as the input vec
        let _ = successful.try_push(SettledDealData {
            deal_id,
            client: deal_proposal.client.clone(),
            provider: deal_proposal.provider.clone(),
            amount: deal_settlement_amount,
        });

        // NOTE(@jmg-duarte,28/06/2024): Maybe emit an event when the table is updated?
        if complete_deal {
            unlock_funds::<T>(&deal_proposal.provider, deal_proposal.provider_collateral)?;
            Proposals::<T>::remove(deal_id);
        } else {
            // Otherwise, we update the proposal — `last_updated_block`
            active_deal_state.last_updated_block = Some(current_block);
            Proposals::<T>::insert(deal_id, deal_proposal);
        }
    }

    Pallet::<T>::deposit_event(Event::<T>::DealsSettled {
        successful,
        unsuccessful,
    });

    Ok(())
}
