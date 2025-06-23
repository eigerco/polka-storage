use frame_system::pallet_prelude::*;
use primitives::{self, deals::DealState};

use crate::{
    slash_and_burn, unlock_funds, BalanceOf, Config, DealsForBlock, Event, Pallet,
    PendingProposals, Proposals, LOG_TARGET,
};

/// When deals are published in [`publish_storage_deals`], they're added to the `DealsForBlock::<T>::get(current_block)` data structure.
/// When they are activated in [`activate_deal`], their state is changed from `DealState::Published` to `DealState::Active`
/// If it did not happen, when [`on_finalize`] reaches `current_block`, it gets Deals that were supposed to be `DealState::Active` from `DealForBlock`.
/// If they are not `DealState::Active`, hook slashes the Storage Provider and returns all of the funds to the Client.
///
/// *This function should not fail at any point, if it fails, it's a bug.*
pub fn slash_providers<T>(current_block: BlockNumberFor<T>)
where
    T: Config,
{
    let deal_ids = DealsForBlock::<T>::get(&current_block);
    if deal_ids.is_empty() {
        log::info!(target: LOG_TARGET, "on_finalize: no deals to process in block: {:?}", current_block);
        return;
    }

    // INVARIANT: every deal in deal_ids is unique.
    // PRE-COND: deal validation has been performed by `publish_storage_deals`.
    let mut pending_proposals = PendingProposals::<T>::get();
    for deal_id in deal_ids {
        let Ok(proposal) = Proposals::<T>::try_get(&deal_id) else {
            // Proposal might have been cleaned up by manual settlement or termination prior to reaching
            // this scheduled block. Nothing more to do for this deal.
            continue;
        };

        match &proposal.state {
            DealState::Published => {
                debug_assert!(
                    proposal.start_block == current_block,
                    "deals are scheduled to be checked only at their start block"
                );

                // Deal has not been activated, time to slash!
                // PRE-COND: deal cannot make to this stage without being validated and proper funds allocated
                let Some(total_storage_fee) = proposal.total_storage_fee() else {
                    log::error!(target: LOG_TARGET, "on_finalize: invariant violated cannot calculate total storage fee, deal {}", deal_id);
                    continue;
                };
                let Ok(client_fee) = TryInto::<BalanceOf<T>>::try_into(total_storage_fee) else {
                    log::error!(target: LOG_TARGET, "on_finalize: invariant violated, cannot convert total storage to {}, deal {}", total_storage_fee, deal_id);
                    continue;
                };

                let Ok(()) = unlock_funds::<T>(&proposal.client, client_fee) else {
                    log::error!(target: LOG_TARGET, "on_finalize: invariant violated, failed to return the fee to the client, deal {}", deal_id);
                    continue;
                };

                log::info!(
                    "on_finalize: slashing {:?} for not activating a deal {}",
                    proposal.provider,
                    deal_id
                );

                let Some(provider_collateral) = proposal.provider_collateral() else {
                    log::error!(target: LOG_TARGET, "on_finalize: invariant violated cannot calculate provider_collateral, deal {}", deal_id);
                    continue;
                };
                let Ok(provider_collateral) =
                    TryInto::<BalanceOf<T>>::try_into(provider_collateral)
                else {
                    log::error!(target: LOG_TARGET, "on_finalize: invariant violated, cannot convert provider_collateral {}, deal {}", provider_collateral, deal_id);
                    continue;
                };

                // PRE-COND: deal MUST BE validated and the proper funds allocated
                let Ok(()) = slash_and_burn::<T>(&proposal.provider, provider_collateral) else {
                    log::error!(target: LOG_TARGET, "on_finalize: invariant violated, cannot slash the deal {}", deal_id);
                    continue;
                };

                Pallet::<T>::deposit_event(Event::<T>::DealSlashed {
                    deal_id,
                    provider: proposal.provider.clone(),
                    client: proposal.client.clone(),
                    amount: provider_collateral,
                });
            }
            DealState::Active(_) => {
                log::info!(
                    "on_finalize: deal {} has been properly activated before, all good.",
                    deal_id
                );
                continue;
            }
        }

        // Deal has been processed, no need to process it twice.
        Proposals::<T>::remove(&deal_id);
        // PRE-COND: all deals in DealsPerBlock are published.
        // All Published deals are hashed and added to [`PendingProposals`].
        let _ = pending_proposals.remove(&Pallet::<T>::hash_proposal(&proposal));
    }

    PendingProposals::<T>::set(pending_proposals);
    DealsForBlock::<T>::remove(&current_block);
}
