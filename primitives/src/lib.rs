#![cfg_attr(not(feature = "std"), no_std)] // no_std by default, requires "std" for std-support

pub mod commitment;
pub mod pallets;
pub mod proofs;
pub mod randomness;

/// Merkle tree node size in bytes.
pub const NODE_SIZE: usize = 32;

/// Max number of sectors.
/// <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/runtime/src/runtime/policy.rs#L262>
pub const MAX_SECTORS: u32 = 32 << 20;
