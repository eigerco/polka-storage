use cid::Cid;
use codec::{Codec, Decode, Encode};
use scale_info::TypeInfo;
use sp_core::{ConstU32, RuntimeDebug};
use sp_runtime::{BoundedBTreeMap, BoundedBTreeSet, BoundedVec, DispatchError, DispatchResult};

use crate::{
    commitment::RawCommitment,
    proofs::{ProverId, PublicReplicaInfo, RegisteredPoStProof, RegisteredSealProof, Ticket},
    sector::SectorNumber,
    DealId, PartitionNumber, MAX_DEALS_PER_SECTOR, MAX_PARTITIONS_PER_DEADLINE,
    MAX_POST_PROOF_BYTES, MAX_SEAL_PROOF_BYTES, MAX_SECTORS, MAX_SECTORS_PER_CALL,
    MAX_SECTORS_PER_PROOF,
};

/// Represents functions that are provided by the Randomness Pallet
pub trait Randomness<BlockNumber> {
    fn get_randomness(block_number: BlockNumber) -> Result<[u8; 32], DispatchError>;
}

pub trait StorageProviderValidation<AccountId> {
    /// Checks that the storage provider is registered.
    fn is_registered_storage_provider(storage_provider: &AccountId) -> bool;
}

/// Entrypoint for proof verification implemented by Pallet Proofs.
pub trait ProofVerification {
    fn verify_porep(
        prover_id: ProverId,
        seal_proof: RegisteredSealProof,
        comm_r: RawCommitment,
        comm_d: RawCommitment,
        sector: SectorNumber,
        ticket: Ticket,
        seed: Ticket,
        proof: BoundedVec<u8, ConstU32<MAX_SEAL_PROOF_BYTES>>,
    ) -> DispatchResult;

    fn verify_post(
        post_type: RegisteredPoStProof,
        randomness: Ticket,
        replicas: BoundedBTreeMap<SectorNumber, PublicReplicaInfo, ConstU32<MAX_SECTORS_PER_PROOF>>,
        proof: BoundedVec<u8, ConstU32<MAX_POST_PROOF_BYTES>>,
    ) -> DispatchResult;
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

/// Current deadline in a proving period of a Storage Provider.
#[derive(Encode, Decode, TypeInfo)]
pub struct DeadlineInfo<BlockNumber> {
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

/// Snapshot information about a deadline. It's partitions and sectors assigned to it.
#[derive(Encode, Decode, TypeInfo)]
pub struct DeadlineState {
    /// Partitions in this deadline. Indexed by partition number.
    pub partitions:
        BoundedBTreeMap<PartitionNumber, PartitionState, ConstU32<MAX_PARTITIONS_PER_DEADLINE>>,
}

#[derive(Encode, Decode, TypeInfo)]
pub struct PartitionState {
    pub sectors: BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,
}

sp_api::decl_runtime_apis! {
    pub trait StorageProviderApi<AccountId> where AccountId: Codec
    {
        /// Gets the information about the specified deadline of the storage provider.
        ///
        /// If there is no Storage Provider of given AccountId returns [`Option::None`].
        /// May exceptionally return [`Option::None`] when
        /// conversion between BlockNumbers fails, but technically should not ever happen.
        fn deadline_info(storage_provider: AccountId, deadline_index: u64) -> Option<
            DeadlineInfo<
                <<Block as sp_runtime::traits::Block>::Header as sp_runtime::traits::Header>::Number
            >
        >;

        /// Returns snapshot information about the deadline, i.e. which sectors are assigned to which partitions.
        /// When the deadline has not opened yet (deadline_start - WPoStChallengeWindow), it can change!
        fn deadline_state(storage_provider: AccountId, deadline_index: u64) -> Option<DeadlineState>;
    }
}
