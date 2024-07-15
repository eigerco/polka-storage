use cid::Cid;
use sp_core::ConstU32;
use sp_runtime::{BoundedVec, DispatchError, DispatchResult, RuntimeDebug};

use crate::types::{DealId, RegisteredSealProof, SectorNumber};

/// Size of a CID with a 512-bit multihash — i.e. the default CID size.
const CID_SIZE_IN_BYTES: u32 = 64;

// TODO(#129,@cernicc,11/07/2024): Refactor to a new-type. SectorId should
// always be a valid CID. That should be checked before initializing it.
/// The CID (in bytes) of a given sector.
pub type SectorId = BoundedVec<u8, ConstU32<CID_SIZE_IN_BYTES>>;

/// Number of Sectors that can be provided in a single extrinsics call.
/// Required for BoundedVec.
/// It was selected arbitrarly, without precise calculations.
pub const MAX_SECTORS_PER_CALL: u32 = 32;
/// Number of Deals that can be contained in a single sector.
/// Required for BoundedVec.
/// It was selected arbitrarly, without precise calculations.
pub const MAX_DEALS_PER_SECTOR: u32 = 128;
/// Flattened size of all active deals for all of the sectors.
/// Required for BoundedVec.
pub const MAX_DEALS_FOR_ALL_SECTORS: u32 = MAX_SECTORS_PER_CALL * MAX_DEALS_PER_SECTOR;

/// Represents functions that are provided by the Market Provider Pallet
pub trait Market<AccountId, BlockNumber> {
    /// Verifies a given set of storage deals is valid for sectors being PreCommitted.
    /// Computes UnsealedCID (CommD) for each sector or None for Committed Capacity sectors.
    fn verify_deals_for_activation(
        storage_provider: &AccountId,
        sector_deals: BoundedVec<SectorDeal<BlockNumber>, ConstU32<MAX_SECTORS_PER_CALL>>,
    ) -> Result<BoundedVec<Option<Cid>, ConstU32<MAX_SECTORS_PER_CALL>>, DispatchError>;

    /// Activate a set of deals grouped by sector, returning the size and
    /// extra info about verified deals.
    /// Sectors' deals are activated in parameter-defined order.
    /// Each sector's deals are activated or fail as a group, but independently of other sectors.
    /// Note that confirming all deals fit within a sector is the caller's responsibility
    /// (and is implied by confirming the sector's data commitment is derived from the deal pieces).
    fn activate_deals(
        storage_provider: &AccountId,
        sector_deals: BoundedVec<SectorDeal<BlockNumber>, ConstU32<MAX_SECTORS_PER_CALL>>,
        compute_cid: bool,
    ) -> Result<BoundedVec<ActiveSector<AccountId>, ConstU32<MAX_SECTORS_PER_CALL>>, DispatchError>;

    /// Terminate a set of deals in response to their sector being terminated.
    ///
    /// Slashes the provider collateral, refunds the partial unpaid escrow amount to the client.
    ///
    /// A sector can be terminated voluntarily — the storage provider terminates the sector —
    /// or involuntarily — the sector has been faulty for more than 42 consecutive days.
    ///
    /// Source: <https://github.com/filecoin-project/builtin-actors/blob/54236ae89880bf4aa89b0dba6d9060c3fd2aacee/actors/market/src/lib.rs#L786-L876>
    fn on_sectors_terminate(
        storage_provider: &AccountId,
        sector_ids: BoundedVec<SectorId, ConstU32<MAX_DEALS_PER_SECTOR>>,
    ) -> DispatchResult;
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
    pub deal_ids: BoundedVec<DealId, ConstU32<MAX_DEALS_PER_SECTOR>>,
}

/// A sector with all of its active deals.
#[derive(RuntimeDebug, Eq, PartialEq)]
pub struct ActiveSector<AccountId> {
    /// Information about each deal activated.
    pub active_deals: BoundedVec<ActiveDeal<AccountId>, ConstU32<MAX_DEALS_PER_SECTOR>>,
    /// Unsealed CID computed from the deals specified for the sector.
    /// A None indicates no deals were specified, or the computation was not requested.
    pub unsealed_cid: Option<Cid>,
}

/// An active deal with references to data that it stores
#[derive(RuntimeDebug, Eq, PartialEq)]
pub struct ActiveDeal<AccountId> {
    /// Client's account
    pub client: AccountId,
    /// Data that was stored
    pub piece_cid: Cid,
    /// Real size of the data
    pub piece_size: u64,
}
