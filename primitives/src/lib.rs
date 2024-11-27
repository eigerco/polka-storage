#![cfg_attr(not(feature = "std"), no_std)] // no_std by default, requires "std" for std-support

pub mod commitment;
pub mod pallets;
pub mod proofs;
pub mod randomness;

/// Merkle tree node size in bytes.
pub const NODE_SIZE: usize = 32;
