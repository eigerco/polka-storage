use cid::Cid;
use sp_core::ConstU32;
use sp_runtime::{BoundedVec, DispatchError, RuntimeDebug};

use crate::types::{DealId, RegisteredSealProof, SectorNumber};

/// Represents functions that are provided by the Market Provider Pallet
pub trait Market<AccountId, BlockNumber> {
    /// Verifies a given set of storage deals is valid for sectors being PreCommitted.
    /// Computes UnsealedCID (CommD) for each sector or None for Committed Capacity sectors.
    fn verify_deals_for_activation(
        storage_provider: &AccountId,
        sector_deals: BoundedVec<SectorDeal<BlockNumber>, ConstU32<32>>,
    ) -> Result<BoundedVec<Option<Cid>, ConstU32<32>>, DispatchError>;
}

/// Binds given Sector with the Deals that it should contain
#[derive(RuntimeDebug)]
pub struct SectorDeal<BlockNumber> {
    pub sector_number: SectorNumber,
    pub sector_expiry: BlockNumber,
    pub sector_type: RegisteredSealProof,
    pub deal_ids: BoundedVec<DealId, ConstU32<128>>,
}
