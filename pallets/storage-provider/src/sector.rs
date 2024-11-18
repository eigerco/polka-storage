use codec::{Decode, Encode};
use frame_support::{pallet_prelude::*, BoundedVec};
use primitives_proofs::{
    DealId, RegisteredSealProof, SectorDeal, SectorNumber, CID_SIZE_IN_BYTES, MAX_DEALS_PER_SECTOR,
    MAX_SEAL_PROOF_BYTES, MAX_TERMINATIONS_PER_CALL,
};
use scale_info::TypeInfo;

use crate::{pallet::DECLARATIONS_MAX, partition::PartitionNumber};

// https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/runtime/src/runtime/policy.rs#L262
pub const MAX_SECTORS: u32 = 32 << 20;

/// This type is passed into the pre commit function on the storage provider pallet
#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq, Eq, TypeInfo)]
pub struct SectorPreCommitInfo<BlockNumber> {
    pub seal_proof: RegisteredSealProof,
    /// Which sector number this SP is pre-committing.
    pub sector_number: SectorNumber,
    /// This value is also known as `commR` or "commitment of replication". The terms `commR` and `sealed_cid` are interchangeable.
    /// Using sealed_cid as I think that is more descriptive.
    /// Some docs on commR here: <https://proto.school/verifying-storage-on-filecoin/03>
    pub sealed_cid: BoundedVec<u8, ConstU32<CID_SIZE_IN_BYTES>>,
    /// The block number at which we requested the randomness when sealing the sector.
    pub seal_randomness_height: BlockNumber,
    /// Deals Ids that are supposed to be activated.
    /// If any of those is invalid, whole activation is rejected.
    pub deal_ids: BoundedVec<DealId, ConstU32<MAX_DEALS_PER_SECTOR>>,
    /// Expiration of the pre-committed sector.
    pub expiration: BlockNumber,
    /// This value is also known as `commD` or "commitment of data".
    /// Once a sector is full `commD` is produced representing the root node of all of the piece CIDs contained in the sector.
    pub unsealed_cid: BoundedVec<u8, ConstU32<CID_SIZE_IN_BYTES>>,
}

/// Information stored on-chain for a pre-committed sector.
#[derive(Clone, RuntimeDebug, Decode, Encode, TypeInfo)]
pub struct SectorPreCommitOnChainInfo<Balance, BlockNumber> {
    pub info: SectorPreCommitInfo<BlockNumber>,
    /// Total collateral for this sector
    pub pre_commit_deposit: Balance,
    /// Block number at which the sector was pre-committed
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
            sector_type: precommit.info.seal_proof,
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
    pub sealed_cid: BoundedVec<u8, ConstU32<CID_SIZE_IN_BYTES>>,
    /// Block number during which the sector proof was accepted
    pub activation: BlockNumber,
    /// Block number during which the sector expires
    pub expiration: BlockNumber,
    /// CommD
    pub unsealed_cid: BoundedVec<u8, ConstU32<CID_SIZE_IN_BYTES>>,
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
#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq, TypeInfo)]
pub struct ProveCommitSector {
    pub sector_number: SectorNumber,
    pub proof: BoundedVec<u8, ConstU32<MAX_SEAL_PROOF_BYTES>>,
}

/// Type that is emitted after a successful prove commit extrinsic.
#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq, TypeInfo)]
pub struct ProveCommitResult {
    /// The sector number that is proven.
    pub sector_number: SectorNumber,
    /// The partition number the proven sector is in.
    pub partition_number: PartitionNumber,
    //// The deadline index assigned to the proven sector.
    pub deadline_idx: u64,
}

impl ProveCommitResult {
    pub fn new(
        sector_number: SectorNumber,
        partition_number: PartitionNumber,
        deadline_idx: u64,
    ) -> Self {
        Self {
            sector_number,
            partition_number,
            deadline_idx,
        }
    }
}

/// Argument used for the `terminate_sectors` extrinsic
#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq, TypeInfo)]
pub struct TerminateSectorsParams {
    pub terminations: BoundedVec<TerminationDeclaration, ConstU32<DECLARATIONS_MAX>>,
}

#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq, TypeInfo)]
pub struct TerminationDeclaration {
    pub deadline: u64,
    pub partition: PartitionNumber,
    pub sectors: BoundedBTreeSet<SectorNumber, ConstU32<MAX_TERMINATIONS_PER_CALL>>,
}
