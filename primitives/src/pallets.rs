use cid::Cid;
use codec::{Codec, Decode, Encode};
use scale_info::TypeInfo;
use sp_core::{ConstU32, RuntimeDebug};
use sp_runtime::{BoundedBTreeMap, BoundedBTreeSet, BoundedVec, DispatchResult};

use crate::{
    commitment::RawCommitment,
    proofs::{ProverId, PublicReplicaInfo, RegisteredPoStProof, RegisteredSealProof, Ticket},
    sector::SectorNumber,
    DealId, PartitionNumber, MAX_DEALS_PER_SECTOR, MAX_PARTITIONS_PER_DEADLINE,
    MAX_POREP_PROOFS_PER_BLOCK, MAX_POST_PROOFS_PER_BLOCK, MAX_POST_PROOF_BYTES,
    MAX_REPLICAS_PER_BLOCK, MAX_SEAL_PROOF_BYTES, MAX_SECTORS,
};

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
        proofs: BoundedVec<
            BoundedVec<u8, ConstU32<MAX_SEAL_PROOF_BYTES>>,
            ConstU32<MAX_POREP_PROOFS_PER_BLOCK>,
        >,
    ) -> DispatchResult;

    fn verify_post(
        post_type: RegisteredPoStProof,
        randomness: Ticket,
        replicas: BoundedBTreeMap<
            SectorNumber,
            PublicReplicaInfo,
            ConstU32<MAX_REPLICAS_PER_BLOCK>,
        >,
        proof: BoundedVec<
            BoundedVec<u8, ConstU32<MAX_POST_PROOF_BYTES>>,
            ConstU32<MAX_POST_PROOFS_PER_BLOCK>,
        >,
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
    /// Block at which the deadline closes.
    pub close: BlockNumber,
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
        /// conversion between BlockNumbers fails, but technically should never happen.
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
