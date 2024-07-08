use codec::{Decode, Encode};
use primitives_proofs::{DealId, SectorId};
use scale_info::TypeInfo;

use crate::proofs::RegisteredSealProof;

// https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/runtime/src/runtime/policy.rs#L262
pub const SECTORS_MAX: u32 = 32 << 20;

/// SectorNumber is a numeric identifier for a sector.
pub type SectorNumber = u32;

/// SectorSize indicates one of a set of possible sizes in the network.
#[derive(Encode, Decode, TypeInfo, Clone, Debug, PartialEq, Eq, Copy)]
pub enum SectorSize {
    _2KiB,
}

/// This type is passed into the pre commit function on the storage provider pallet
#[derive(Clone, Debug, Decode, Encode, PartialEq, TypeInfo)]
pub struct SectorPreCommitInfo<BlockNumber> {
    pub seal_proof: RegisteredSealProof,
    /// Which sector number this SP is pre-committing.
    pub sector_number: SectorNumber,
    /// This value is also known as 'commR', Commitment of replication. The terms commR and sealed_cid are interchangeable.
    /// Using sealed_cid as I think that is more descriptive.
    /// Some docs on commR here: <https://proto.school/verifying-storage-on-filecoin/03>
    pub sealed_cid: SectorId,
    pub deal_id: DealId,
    /// Expiration of the pre-committed sector.
    pub expiration: BlockNumber,
    /// CommD
    pub unsealed_cid: SectorId,
}

/// Information stored on-chain for a pre-committed sector.
#[derive(Debug, Decode, Encode, TypeInfo)]
pub struct SectorPreCommitOnChainInfo<Balance, BlockNumber> {
    pub info: SectorPreCommitInfo<BlockNumber>,
    /// Total collateral for this sector
    pub pre_commit_deposit: Balance,
    /// Block number this was pre-committed
    pub pre_commit_block_number: BlockNumber,
}

impl<Balance, BlockNumber> SectorPreCommitOnChainInfo<Balance, BlockNumber> {
    pub fn new(
        info: SectorPreCommitInfo<BlockNumber>,
        pre_commit_deposit: Balance,
        pre_commit_block_number: BlockNumber,
    ) -> Self {
        Self {
            info,
            pre_commit_deposit,
            pre_commit_block_number,
        }
    }
}

#[derive(Debug, Decode, Encode, TypeInfo)]
pub struct SectorOnChainInfo<BlockNumber> {
    pub sector_number: SectorNumber,
    /// The seal proof type implies the PoSt proofs
    pub seal_proof: RegisteredSealProof,
    /// The root hash of the sealed sector's merkle tree.
    /// This value is also known as 'commR', Commitment of replication. The terms commR and sealed_cid are interchangeable.
    /// Using sealed_cid as I think that is more descriptive.
    /// Some docs on commR here: <https://proto.school/verifying-storage-on-filecoin/03>
    pub sealed_cid: SectorId,
    /// Block number during which the sector proof was accepted
    pub activation: BlockNumber,
    /// Block number during which the sector expires
    pub expiration: BlockNumber,
    /// CommD
    pub unsealed_cid: SectorId,
}
