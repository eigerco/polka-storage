// mod chunker;
// mod pb;
mod v1;
mod v2;

use ipld_core::cid::multihash::Multihash;
use sha2::{Digest, Sha256};

// TODO(@jmg-duarte,20/05/2024): move and unify the Error here

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
