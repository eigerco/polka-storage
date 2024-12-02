#![cfg_attr(not(feature = "std"), no_std)] // no_std by default, requires "std" for std-support

pub mod commitment;
pub mod pallets;
pub mod proofs;
pub mod randomness;
pub mod sector;

pub type DealId = u64;

pub type PartitionNumber = u32;

/// Merkle tree node size in bytes.
pub const NODE_SIZE: usize = 32;

/// Max amount of partitions per deadline.
/// ref: <https://github.com/filecoin-project/builtin-actors/blob/82d02e58f9ef456aeaf2a6c737562ac97b22b244/runtime/src/runtime/policy.rs#L283>
pub const MAX_PARTITIONS_PER_DEADLINE: u32 = 3000;

/// Max number of sectors.
/// <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/runtime/src/runtime/policy.rs#L262>
pub const MAX_SECTORS: u32 = 32 << 20;

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
