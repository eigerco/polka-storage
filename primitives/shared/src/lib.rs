#![no_std]

pub mod commcid;
pub mod piece;

/// Merkle tree node size in bytes.
/// TODO: Where should this be moved to?
pub const NODE_SIZE: usize = 32;
