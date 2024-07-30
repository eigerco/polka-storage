use codec::{Decode, Encode};
use frame_support::{pallet_prelude::*, BoundedVec};
use primitives_proofs::{
    DealId, RegisteredSealProof, SectorDeal, SectorId, SectorNumber, MAX_DEALS_PER_SECTOR,
};
use scale_info::TypeInfo;

// https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/runtime/src/runtime/policy.rs#L262
pub const MAX_SECTORS: u32 = 32 << 20;

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
    /// Deals Ids that are supposed to be activated.
    /// If any of those is invalid, whole activation is rejected.
    pub deal_ids: BoundedVec<DealId, ConstU32<MAX_DEALS_PER_SECTOR>>,
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

impl<Balance, BlockNumber> From<&SectorPreCommitOnChainInfo<Balance, BlockNumber>>
    for SectorDeal<BlockNumber>
where
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    fn from(precommit: &SectorPreCommitOnChainInfo<Balance, BlockNumber>) -> Self {
        Self {
            sector_number: precommit.info.sector_number,
            sector_expiry: precommit.info.expiration,
            sector_type: precommit.info.seal_proof.clone(),
            deal_ids: precommit.info.deal_ids.clone(),
        }
    }
}

#[derive(Clone, Decode, Encode, TypeInfo, RuntimeDebug)]
pub struct SectorOnChainInfo<BlockNumber>
where
    BlockNumber: sp_runtime::traits::BlockNumber,
{
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

impl<BlockNumber> SectorOnChainInfo<BlockNumber>
where
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    pub fn from_pre_commit(
        pre_commit: SectorPreCommitInfo<BlockNumber>,
        activation: BlockNumber,
    ) -> Self {
        SectorOnChainInfo {
            sector_number: pre_commit.sector_number,
            seal_proof: pre_commit.seal_proof,
            sealed_cid: pre_commit.sealed_cid,
            expiration: pre_commit.expiration,
            activation,
            unsealed_cid: pre_commit.unsealed_cid,
        }
    }
}

/// Arguments passed into the `prove_commit_sector` extrinsic.
#[derive(Clone, Debug, Decode, Encode, PartialEq, TypeInfo)]
pub struct ProveCommitSector {
    pub sector_number: SectorNumber,
    pub proof: BoundedVec<u8, ConstU32<256>>, // Arbitrary length
}
