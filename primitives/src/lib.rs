#![cfg_attr(not(feature = "std"), no_std)] // no_std by default, requires "std" for std-support

pub mod commitment;
pub mod proofs;

/// Merkle tree node size in bytes.
pub const NODE_SIZE: usize = 32;
