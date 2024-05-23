//! Multicodec utilities, such as the list of codes,
//! as per the [code table](https://github.com/multiformats/multicodec/blob/c954a787dc6a17d099653e5f90d26fbd177d2074/table.csv).
pub const SHA_256_CODE: u64 = 0x12;
pub const SHA_512_CODE: u64 = 0x13;
pub const RAW_CODE: u64 = 0x55;
pub const DAG_PB_CODE: u64 = 0x70;

// This is easier than adding #[cfg(test)] to each one
#[cfg(test)]
mod private {
    use digest::Digest;
    use ipld_core::cid::multihash::Multihash;

    use super::{SHA_256_CODE, SHA_512_CODE};

    /// Trait to ease implementing generic multihash generation.
    pub(crate) trait MultihashCode {
        /// Multihash code as defined in the [specification](https://github.com/multiformats/multicodec/blob/c954a787dc6a17d099653e5f90d26fbd177d2074/table.csv).
        const CODE: u64;
    }

    impl MultihashCode for sha2::Sha256 {
        const CODE: u64 = SHA_256_CODE;
    }

    impl MultihashCode for sha2::Sha512 {
        const CODE: u64 = SHA_512_CODE;
    }

    /// Generate a multihash for a byte slice.
    pub(crate) fn generate_multihash<H>(bytes: &[u8]) -> Multihash<64>
    where
        H: Digest + MultihashCode,
    {
        let mut hasher = H::new();
        hasher.update(&bytes);
        let hashed_bytes = hasher.finalize();
        Multihash::wrap(H::CODE, &hashed_bytes).unwrap()
    }
}

#[cfg(test)]
pub(crate) use private::*;
