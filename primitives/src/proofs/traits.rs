extern crate alloc;

use cid::Cid;
use sp_core::ConstU32;
use sp_runtime::{BoundedBTreeMap, BoundedVec, DispatchError, DispatchResult, RuntimeDebug};

use super::types::{ProverId, RegisteredPoStProof, RegisteredSealProof, Ticket};
use crate::{commitment::RawCommitment, sector::SectorNumber};

/// Size of a CID with a 512-bit multihash — i.e. the size of CommR/CommD/CommP
pub const CID_SIZE_IN_BYTES: u32 = 64;

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

/// The maximum number of terminations for a single extrinsic call.
pub const MAX_TERMINATIONS_PER_CALL: u32 = 32; // TODO(@jmg-duarte,25/07/2024): change for a better value

/// The maximum amount of sectors allowed in proofs and replicas.
/// This value is the absolute max, when the sector size is 32 GiB.
/// Proofs and replicas are still dynamically checked for their size depending on the sector size.
///
/// References:
/// * Filecoin docs about PoSt: <https://spec.filecoin.io/algorithms/pos/post/#section-algorithms.pos.post.windowpost>
pub const MAX_SECTORS_PER_PROOF: u32 = 2349;

/// The absolute maximum length, in bytes, a seal proof should be for the largest sector size.
/// NOTE: Taken the value from `StackedDRG32GiBV1`,
/// which is not the biggest seal proof type but we do not plan on supporting non-interactive proof types at this time.
///
/// References:
/// * <https://github.com/filecoin-project/ref-fvm/blob/32583cc05aa422c8e1e7ba81d56a888ac9d90e61/shared/src/sector/registered_proof.rs#L90>
pub const MAX_SEAL_PROOF_BYTES: u32 = 1_920;

/// The fixed length, in bytes, of a PoSt proof.
/// This value is the same as `PROOF_BYTES` in the `polka-storage-proofs` library.
/// It is redefined to avoid import the whole library for 1 constant.
///
/// References:
/// * <https://github.com/filecoin-project/ref-fvm/blob/32583cc05aa422c8e1e7ba81d56a888ac9d90e61/shared/src/sector/registered_proof.rs#L159>
pub const MAX_POST_PROOF_BYTES: u32 = 192;

/// The minimal information required about a replica, in order to be able to verify
/// a PoSt over it.
#[derive(Clone, core::fmt::Debug, PartialEq, Eq)]
pub struct PublicReplicaInfo {
    /// The replica commitment.
    pub comm_r: RawCommitment,
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
