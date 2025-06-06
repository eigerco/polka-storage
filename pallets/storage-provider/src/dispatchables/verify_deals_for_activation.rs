use cid::Cid;
use frame_support::pallet_prelude::ConstU32;
use frame_system::Pallet as System;
use primitives::{pallets::SectorDeal, MAX_DEALS_PER_SECTOR};
use sp_runtime::{BoundedVec, DispatchError};

use super::{compute_commd, proposals_for_deals, validate_deals_for_sector};
use crate::{dispatchables::BlockNumberFor, Config};

pub fn verify_deals_for_activation<T>(
    storage_provider: &T::AccountId,
    sector_deals: BoundedVec<SectorDeal<BlockNumberFor<T>>, ConstU32<MAX_DEALS_PER_SECTOR>>,
) -> Result<BoundedVec<Option<Cid>, ConstU32<MAX_DEALS_PER_SECTOR>>, DispatchError>
where
    T: Config,
{
    let curr_block = System::<T>::block_number();
    let mut unsealed_cids = BoundedVec::new();
    for sector in sector_deals {
        let proposals = proposals_for_deals::<T>(sector.deal_ids)?;
        let sector_size = sector.sector_type.sector_size();
        validate_deals_for_sector::<T>(
            &proposals,
            storage_provider,
            sector.sector_number,
            sector.sector_expiry,
            curr_block,
            sector_size,
        )?;

        // Sealing a Sector without Deals, Committed Capacity Only.
        let commd = if proposals.is_empty() {
            None
        } else {
            Some(compute_commd::<T>(
                proposals.iter().map(|(_, deal)| deal),
                sector.sector_type,
            )?)
        };

        // PRE-COND: can't fail, unsealed_cids<_, X> == BoundedVec<_ X> == sector_deals<_, X>
        unsealed_cids
            .try_push(commd)
            .map_err(|_| "programmer error, there should be space for Cids")?;
    }

    Ok(unsealed_cids)
}
