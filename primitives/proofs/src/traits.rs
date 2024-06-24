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

    /// Activate a set of deals grouped by sector, returning the size and
    /// extra info about verified deals.
    /// Sectors' deals are activated in parameter-defined order.
    /// Each sector's deals are activated or fail as a group, but independently of other sectors.
    /// Note that confirming all deals fit within a sector is the caller's responsibility
    /// (and is implied by confirming the sector's data commitment is derived from the deal peices).
    fn activate_deals(
        storage_provider: &AccountId,
        sector_deals: BoundedVec<SectorDeal<BlockNumber>, ConstU32<32>>,
        compute_cid: bool,
    ) -> Result<BoundedVec<ActivatedSector<AccountId>, ConstU32<32>>, DispatchError>;
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

#[derive(RuntimeDebug, Eq, PartialEq)]
pub struct ActivatedSector<AccountId> {
    /// Information about each deal activated.
    pub activated_deals: BoundedVec<ActivatedDeal<AccountId>, ConstU32<128>>,
    /// Unsealed CID computed from the deals specified for the sector.
    /// A None indicates no deals were specified, or the computation was not requested.
    pub unsealed_cid: Option<Cid>,
}

#[derive(RuntimeDebug, Eq, PartialEq)]
pub struct ActivatedDeal<AccountId> {
    pub client: AccountId,
    pub piece_cid: Cid,
    pub piece_size: u64,
}
