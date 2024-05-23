#![warn(unused_crate_dependencies)]
#![warn(rustdoc::broken_intra_doc_links)]

mod v1;
mod v2;

pub use v1::{Header as CarV1Header, Reader as CarV1Reader, Writer as CarV1Writer};
pub use v2::{
    Characteristics, Header as CarV2Header, Index, IndexEntry, IndexSorted, MultihashIndexSorted,
    Reader as CarV2Reader, SingleWidthIndex, Writer as CarV2Writer,
};

// We need to expose this because `read_block` returns `(Cid, Vec<u8>)`.
pub use ipld_core::cid::Cid;

/// CAR handling errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Returned when a version was expected, but another was received.
    ///
    /// For example, when reading CARv1 files, the only valid version is 1,
    /// otherwise, this error should be returned.
    #[error("expected version {expected}, but received version {received} instead")]
    VersionMismatchError { expected: u8, received: u8 },

    /// According to the [specification](https://ipld.io/specs/transport/car/carv1/#constraints)
    /// CAR files MUST have **one or more** CID roots.
    #[error("CAR file must have roots")]
    EmptyRootsError,

    /// Unknown type of index. Supported indexes are
    /// [`IndexSorted`] and [`MultihashIndexSorted`].
    #[error("unknown index type {0}")]
    UnknownIndexError(u64),

    /// Digest does not match the expected length.
    #[error("digest has length {received}, instead of {expected}")]
    NonMatchingDigestError { expected: usize, received: usize },

    /// Cannot know width or count from an empty vector.
    #[error("cannot create an index out of an empty `Vec`")]
    EmptyIndexError,

    /// The [specification](https://ipld.io/specs/transport/car/carv2/#characteristics)
    /// does not discuss how to handle unknown characteristics
    /// — i.e. if we should ignore them, truncate them or return an error —
    /// we decided to return an error when there are unknown bits set.
    #[error("unknown characteristics were set: {0}")]
    UnknownCharacteristicsError(u128),

    /// According to the [specification](https://ipld.io/specs/transport/car/carv2/#pragma)
    /// the pragma is composed of a pre-defined list of bytes,
    /// if the received pragma is not the same, we return an error.
    #[error("received an invalid pragma: {0:?}")]
    InvalidPragmaError(Vec<u8>),

    /// See [`CodecError`](serde_ipld_dagcbor::error::CodecError) for more information.
    #[error(transparent)]
    CodecError(#[from] serde_ipld_dagcbor::error::CodecError),

    /// See [`IoError`](tokio::io::Error) for more information.
    #[error(transparent)]
    IoError(#[from] tokio::io::Error),

    /// See [`CidError`](ipld_core::cid::Error) for more information.
    #[error(transparent)]
    CidError(#[from] ipld_core::cid::Error),

    /// See [`MultihashError`](ipld_core::cid::multihash::Error) for more information.
    #[error(transparent)]
    MultihashError(#[from] ipld_core::cid::multihash::Error),
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
