use codec::{Decode, Encode};
use frame_support::{pallet_prelude::*, BoundedVec};
use primitives::{
    pallets::SectorDeal,
    proofs::RegisteredSealProof,
    sector::{SectorNumber, SectorPreCommitInfo},
    PartitionNumber, CID_SIZE_IN_BYTES, MAX_TERMINATIONS_PER_CALL,
};
use scale_info::TypeInfo;

use crate::pallet::DECLARATIONS_MAX;

/// Information stored on-chain for a pre-committed sector.
#[derive(Clone, RuntimeDebug, Decode, Encode, TypeInfo)]
pub struct SectorPreCommitOnChainInfo<Balance, BlockNumber> {
    pub info: SectorPreCommitInfo<BlockNumber>,
    /// Total collateral for this sector
    pub pre_commit_deposit: Balance,
    /// Block number at which the sector was pre-committed
    pub pre_commit_block_number: BlockNumber,
    /// Seal Randomness
    pub seal_randomness: [u8; 32],
}

impl<Balance, BlockNumber> SectorPreCommitOnChainInfo<Balance, BlockNumber> {
    pub fn new(
        info: SectorPreCommitInfo<BlockNumber>,
        pre_commit_deposit: Balance,
        pre_commit_block_number: BlockNumber,
        seal_randomness: [u8; 32],
    ) -> Self {
        Self {
            info,
            pre_commit_deposit,
            pre_commit_block_number,
            seal_randomness,
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
