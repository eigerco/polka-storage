mod v1;
mod v2;

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

    /// Digest does not match the expected length.
    #[error("digest has length {received}, instead of {expected}")]
    NonMatchingDigestError { expected: usize, received: usize },

    /// Cannot know width or count from an empty vector.
    #[error("cannot create an index out of an empty `Vec`")]
    EmptyIndexError,
}

#[cfg(test)]
mod multihash {
    use digest::Digest;
    use ipld_core::cid::multihash::Multihash;

    /// Trait to ease implementing generic multihash generation.
    pub(crate) trait MultihashCode {
        /// Multihash code as defined in the [specification](https://github.com/multiformats/multicodec/blob/c954a787dc6a17d099653e5f90d26fbd177d2074/table.csv).
        const CODE: u64;
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

    impl MultihashCode for sha2::Sha256 {
        const CODE: u64 = 0x12;
    }

    impl MultihashCode for sha2::Sha512 {
        const CODE: u64 = 0x13;
    }
}
