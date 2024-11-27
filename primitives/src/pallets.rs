use cid::Cid;
use codec::{Codec, Decode, Encode};
use scale_info::TypeInfo;
use sp_core::{ConstU32, RuntimeDebug};
use sp_runtime::{BoundedVec, DispatchError, DispatchResult};

use crate::{
    proofs::{ActiveSector, DealId, RegisteredSealProof},
    sector::SectorNumber,
    MAX_DEALS_PER_SECTOR, MAX_SECTORS_PER_CALL,
};

/// Represents functions that are provided by the Randomness Pallet
pub trait Randomness<BlockNumber> {
    fn get_randomness(block_number: BlockNumber) -> Result<[u8; 32], DispatchError>;
}

pub trait StorageProviderValidation<AccountId> {
    /// Checks that the storage provider is registered.
    fn is_registered_storage_provider(storage_provider: &AccountId) -> bool;
}

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
        sectors: BoundedVec<SectorNumber, ConstU32<MAX_DEALS_PER_SECTOR>>,
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

/// Current deadline in a proving period of a Storage Provider.
#[derive(Encode, Decode, TypeInfo)]
pub struct CurrentDeadline<BlockNumber> {
    /// Index of a deadline.
    ///
    /// If there are 10 deadlines if the proving period, values will be [0, 9].
    /// After proving period rolls over, it'll start from 0 again.
    pub deadline_index: u64,
    /// Whether the deadline is open.
    /// Only is false when `current_block < sp.proving_period_start`.
    pub open: bool,
    /// [`pallet_storage_provider::DeadlineInfo::challenge`].
    ///
    /// Block at which the randomness should be fetched to generate/verify Post.
    pub challenge_block: BlockNumber,
    /// Block at which the deadline opens.
    pub start: BlockNumber,
}

sp_api::decl_runtime_apis! {
    pub trait StorageProviderApi<AccountId> where AccountId: Codec
    {
        /// Gets the current deadline of the storage provider.
        ///
        /// If there is no Storage Provider of given AccountId returns [`Option::None`].
        /// May exceptionally return [`Option::None`] when
        /// conversion between BlockNumbers fails, but technically should not ever happen.
        fn current_deadline(storage_provider: AccountId) -> Option<
            CurrentDeadline<
                <<Block as sp_runtime::traits::Block>::Header as sp_runtime::traits::Header>::Number
            >
        >;
    }
}
