use codec::{Decode, Encode};
use scale_info::TypeInfo;

use crate::{proofs::RegisteredSealProof, Cid};

// https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/runtime/src/runtime/policy.rs#L262
pub const SECTORS_MAX: u32 = 32 << 20;

/// SectorNumber is a numeric identifier for a sector.
pub type SectorNumber = u64;

/// SectorSize indicates one of a set of possible sizes in the network.
#[derive(Encode, Decode, TypeInfo, Clone, Debug, PartialEq, Eq, Copy)]
pub enum SectorSize {
    _2KiB,
}

/// This type is passed into the pre commit function on the storage provider pallet
#[derive(Debug, Decode, Encode, TypeInfo)]
pub struct SectorPreCommitInfo {
    pub seal_proof: RegisteredSealProof,
    pub sector_number: SectorNumber,
    pub sealed_cid: Cid,
    pub expiration: u64,
}

/// Information stored on-chain for a pre-committed sector.
#[derive(Debug, Decode, Encode, TypeInfo)]
pub struct SectorPreCommitOnChainInfo<Balance, BlockNumber> {
    pub info: SectorPreCommitInfo,
    /// Total collateral for this sector
    pub pre_commit_deposit: Balance,
    /// Block number this was pre-committed
    pub pre_commit_block_number: BlockNumber,
}

#[derive(Debug, Decode, Encode, TypeInfo)]
pub struct SectorOnChainInfo<BlockNumber> {
    pub sector_number: SectorNumber,
    /// The seal proof type implies the PoSt proofs
    pub seal_proof: RegisteredSealProof,
    /// The root hash of the sealed sector's merkle tree.
    /// Also called CommR, or 'replica commitment'.
    pub sealed_cid: Cid,
    /// Block number during which the sector proof was accepted
    pub activation: BlockNumber,
    /// Block number during which the sector expires
    pub expiration: BlockNumber,
}
