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
/// It's used as a data transfer object for extrinsics `verify_deals_for_activation`
/// as well as `activate deals`.
/// It represents a sector that should be activated and it's deals.
#[derive(RuntimeDebug)]
pub struct SectorDeal<BlockNumber> {
    /// Number of the sector that is supposed to contain the deals
    pub sector_number: SectorNumber,
    /// Time when the sector expires.
    /// If sector expires before some of the deals end, than it's violation and sector is rejected.
    pub sector_expiry: BlockNumber,
    /// Used to extract the size of a sector
    /// All of the deals must fit within the seal proof's sector size.
    /// If not, sector is rejected.
    pub sector_type: RegisteredSealProof,
    /// Deals Ids that are supposed to be activated.
    /// If any of those is invalid, whole activation is rejected.
    pub deal_ids: BoundedVec<DealId, ConstU32<128>>,
}