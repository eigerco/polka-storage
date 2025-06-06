use frame_support::pallet_prelude::*;
use frame_system::{pallet_prelude::*, Pallet as System};
use primitives::{
    deals::{ActiveDealState, DealState},
    pallets::{ActiveDeal, ActiveSector, SectorDeal},
    MAX_DEALS_PER_SECTOR,
};

use super::compute_commd;
use crate::{
    dispatchables::{proposals_for_deals, validate_deals_for_sector},
    Config, Error, Event, Pallet, PendingProposals, Proposals, SectorDeals,
};

pub fn activate_deals<T>(
    storage_provider: &T::AccountId,
    sector_deals: BoundedVec<SectorDeal<BlockNumberFor<T>>, ConstU32<MAX_DEALS_PER_SECTOR>>,
    compute_cid: bool,
) -> Result<BoundedVec<ActiveSector<T::AccountId>, ConstU32<MAX_DEALS_PER_SECTOR>>, DispatchError>
where
    T: Config,
{
    let mut activations = BoundedVec::new();
    let curr_block = System::<T>::block_number();

    let mut pending_proposals = PendingProposals::<T>::get();
    for sector in sector_deals {
        let mut sector_activated_deal_ids: BoundedVec<u64, ConstU32<MAX_DEALS_PER_SECTOR>> =
            BoundedVec::new();

        let Ok(proposals) = proposals_for_deals::<T>(sector.deal_ids) else {
            log::error!("failed to find deals for sector: {}", sector.sector_number);
            continue;
        };

        let sector_size = sector.sector_type.sector_size();
        if let Err(e) = validate_deals_for_sector::<T>(
            &proposals,
            storage_provider,
            sector.sector_number,
            sector.sector_expiry,
            curr_block,
            sector_size,
        ) {
            log::error!(
                "failed to activate sector: {}, skipping... {:?}",
                sector.sector_number,
                e
            );
            continue;
        }

        let data_commitment = if compute_cid && !proposals.is_empty() {
            Some(compute_commd::<T>(
                proposals.iter().map(|(_, deal)| deal),
                sector.sector_type,
            )?)
        } else {
            None
        };

        let mut activated_deals: BoundedVec<_, ConstU32<MAX_DEALS_PER_SECTOR>> = BoundedVec::new();
        for (deal_id, mut proposal) in proposals {
            // Make it Active! This is what's this function is about in the end.
            pending_proposals.remove(&Pallet::<T>::hash_proposal(&proposal));
            proposal.state =
                DealState::Active(ActiveDealState::new(sector.sector_number, curr_block));

            activated_deals
                .try_push(ActiveDeal {
                    client: proposal.client.clone(),
                    piece_cid: proposal
                        .piece_commitment()
                        .map_err(|e| {
                            log::error!(
                                "there is invalid cid saved on-chain for deal: {}, {:?}",
                                deal_id,
                                e
                            );
                            Error::<T>::DealPreconditionFailed
                        })?
                        .cid(),
                    piece_size: proposal.piece_size,
                })
                .map_err(|_| {
                    log::error!("failed to insert into `activated`, programmer's error");
                    Error::<T>::DealPreconditionFailed
                })?;
            sector_activated_deal_ids.try_push(deal_id).map_err(|_| {
                log::error!("failed to insert into `activated_deal_ids`, programmer's error");
                Error::<T>::DealPreconditionFailed
            })?;

            Pallet::<T>::deposit_event(Event::<T>::DealActivated {
                deal_id,
                client: proposal.client.clone(),
                provider: proposal.provider.clone(),
            });
            Proposals::<T>::insert(deal_id, proposal);
        }

        // Insert activated deals for a sector
        SectorDeals::<T>::insert(
            (storage_provider.clone(), sector.sector_number),
            sector_activated_deal_ids,
        );

        activations
            .try_push(ActiveSector {
                active_deals: activated_deals,
                unsealed_cid: data_commitment,
            })
            .map_err(|_| Error::<T>::DealPreconditionFailed)?;
    }

    PendingProposals::<T>::set(pending_proposals);
    Ok(activations)
}
