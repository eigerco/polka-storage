// mod chunker;
// mod pb;
mod v1;
mod v2;

use ipld_core::cid::multihash::Multihash;
use sha2::{Digest, Sha256};

/// Trait to ease implementing generic multihash generation.
pub trait MultihashCode {
    /// Multihash code as defined in the [specification](https://github.com/multiformats/multicodec/blob/c954a787dc6a17d099653e5f90d26fbd177d2074/table.csv).
    const CODE: u64;
}

/// Generate a multihash for a byte slice.
pub fn generate_multihash<H>(bytes: &[u8]) -> Multihash<64>
where
    H: Digest + MultihashCode,
{
    let mut hasher = H::new();
    hasher.update(&bytes);
    let hashed_bytes = hasher.finalize();
    Multihash::wrap(H::CODE, &hashed_bytes).unwrap()
}

impl MultihashCode for Sha256 {
    const CODE: u64 = 0x12;
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    CodecError(#[from] serde_ipld_dagcbor::error::CodecError),

    #[error(transparent)]
    IoError(#[from] tokio::io::Error),

    #[error(transparent)]
    CidError(#[from] ipld_core::cid::Error),

    #[error(transparent)]
    MultihashError(#[from] ipld_core::cid::multihash::Error),

    #[error("trying to read V2")]
    CarV2Error,

    /// Unknown type of index. Supported indexes are
    /// [`MultiWidthIndex`](`crate::car::v2::MultiWidthIndex`) and
    /// [`MultihashIndexSorted`](`crate::car::v2::MultihashIndexSorted`).
    #[error("unknown index type {0}")]
    UnknownIndexError(u64),
}
