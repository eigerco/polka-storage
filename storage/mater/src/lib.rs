mod v1;
mod v2;

pub use v1::{Header as CarV1Header, Reader as CarV1Reader, Writer as CarV1Writer};
pub use v2::{
    Characteristics, Header as CarV2Header, Index, IndexEntry, MultiWidthIndex,
    MultihashIndexSorted, Reader as CarV2Reader, SingleWidthIndex, Writer as CarV2Writer,
};

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

    #[error(
        "invalid version, expected version {expected}, but received version {received} instead"
    )]
    VersionMismatchError { expected: u8, received: u8 },

    /// According to the [specification](https://ipld.io/specs/transport/car/carv1/#constraints)
    /// CAR files MUST have **one or more** CID roots.
    #[error("CAR file must have roots")]
    EmptyRootsError,

    /// Unknown type of index. Supported indexes are
    /// [`MultiWidthIndex`](`crate::v2::MultiWidthIndex`) and
    /// [`MultihashIndexSorted`](`crate::v2::MultihashIndexSorted`).
    #[error("unknown index type {0}")]
    UnknownIndexError(u64),

    /// Digest does not match the expected length.
    #[error("digest has length {received}, instead of {expected}")]
    NonMatchingDigestError { expected: usize, received: usize },

    /// Cannot know width or count from an empty vector.
    #[error("cannot create an index out of an empty `Vec`")]
    EmptyIndexError,

    #[error("unknown characteristics were set: {0}")]
    UnknownCharacteristicsError(u128),
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
