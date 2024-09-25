//! This is a basement....
#![cfg(feature = "std")]

mod types;

use std::path::PathBuf;

use storage_proofs_core::merkle::BinaryMerkleTree;

pub use crate::filecoin::types::*;

/// TODO
/// References:
/// - <https://github.com/eigerco/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-proofs/src/pieces.rs#L85>
/// - <https://github.com/eigerco/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-porep/src/stacked/circuit/proof.rs#L215>
pub fn compute_comm_d(sector_size: SectorSize, piece_info: &[PieceInfo]) -> Result<Commitment, ()> {
    todo!()
}

/// TODO
/// References:
/// - <https://github.com/eigerco/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-porep/src/stacked/circuit/proof.rs#L218>
/// - <https://github.com/eigerco/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-porep/src/stacked/vanilla/proof.rs#L1743>
pub fn compute_comm_r(
    graph: &StackedBucketGraph,
    num_layers: usize,
    data: &[u8], // instead of: mut data: Data<'_>, ???
    data_tree: Option<BinaryMerkleTree<TheHasher>>, // why optional?
    // The directory where the files we operate on are stored.
    // cache_path: PathBuf, // do we need a cache path in our implementation?
    replica_path: PathBuf, // sufficient instead of duplicate location in Data?
    label_configs: Labels,
) -> Result<Commitment, ()> {
    todo!()
}
