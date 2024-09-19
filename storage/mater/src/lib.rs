//! A library to handle CAR files.
//! Both version 1 and version 2 are supported.
//!
//! You can make use of the lower-level utilities such as [`CarV2Reader`] to read a CARv2 file,
//! though these utilities were designed to be used in higher-level abstractions, like the [`Blockstore`].

#![warn(unused_crate_dependencies)]
#![warn(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![deny(unsafe_code)]

mod async_varint;
mod multicodec;
mod stores;
mod unixfs;
mod v1;
mod v2;

// We need to re-expose this because `read_block` returns `(Cid, Vec<u8>)`.
pub use ipld_core::cid::Cid;
pub use stores::{create_filestore, Blockstore, Config};
pub use v1::{Header as CarV1Header, Reader as CarV1Reader, Writer as CarV1Writer};
pub use v2::{
    verify_cid, Characteristics, Header as CarV2Header, Index, IndexEntry, IndexSorted,
    MultihashIndexSorted, Reader as CarV2Reader, SingleWidthIndex, Writer as CarV2Writer,
};

/// CAR handling errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Returned when a version was expected, but another was received.
    ///
    /// For example, when reading CARv1 files, the only valid version is 1,
    /// otherwise, this error should be returned.
    #[error("expected version {expected}, but received version {received} instead")]
    VersionMismatchError {
        /// Expected version (usually 1 or 2)
        expected: u8,
        /// Received version
        received: u8,
    },

    /// According to the [specification](https://ipld.io/specs/transport/car/carv1/#constraints)
    /// CAR files MUST have **one or more** [`Cid`] roots.
    /// This may happen if the input is empty.
    #[error("CAR file must have roots")]
    EmptyRootsError,

    /// Unknown type of index. Supported indexes are
    /// [`IndexSorted`] and [`MultihashIndexSorted`].
    #[error("unknown index type {0}")]
    UnknownIndexError(u64),

    /// Digest does not match the expected length.
    #[error("digest has length {received}, instead of {expected}")]
    NonMatchingDigestError {
        /// Expected digest length
        expected: usize,
        /// Received digest length
        received: usize,
    },

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

    /// See [`ProtobufError`](quick_protobuf::Error) for more information.
    #[error(transparent)]
    ProtobufError(#[from] quick_protobuf::Error),

    /// See [`DagPbError`](ipld_dagpb::Error) for more information.
    #[error(transparent)]
    DagPbError(#[from] ipld_dagpb::Error),
}

#[cfg(test)]
pub(crate) mod test_utils {
    /// Check if two given slices are equal.
    ///
    /// First checks if the two slices have the same size,
    /// then checks each byte-pair. If the slices differ,
    /// it will show an error message with the difference index
    /// along with a window showing surrounding elements
    /// (instead of spamming your terminal like `assert_eq!` does).
    macro_rules! assert_buffer_eq {
        ($left:expr, $right:expr $(,)?) => {{
            assert_eq!($left.len(), $right.len());
            for (i, (l, r)) in $left.iter().zip($right).enumerate() {
                let before = i.checked_sub(5).unwrap_or(0);
                let after = (i + 5).min($right.len());
                assert_eq!(
                    l,
                    r,
                    "difference at index {}\n  left: {:02x?}\n right: {:02x?}",
                    i,
                    &$left[before..=after],
                    &$right[before..=after],
                )
            }
        }};
    }
    use std::path::Path;

    pub(crate) use assert_buffer_eq;
    /// This is here so that our build doesn't fail. It thinks that the
    /// criterion is not used. But it is used by the benchmarks.
    use criterion as _;
    use tokio::{fs::File, io::AsyncWriteExt};

    /// Dump a byte slice into a file.
    ///
    /// * If *anything* goes wrong, the function will panic.
    /// * If the file doesn't exist, it will be created.
    /// * If the file exists, it will be overwritten and truncated.
    #[allow(dead_code)] // This function is supposed to be a debugging helper
    pub(crate) async fn dump<P, B>(path: P, bytes: B)
    where
        P: AsRef<Path>,
        B: AsRef<[u8]>,
    {
        let mut file = File::create(path).await.unwrap();
        file.write_all(bytes.as_ref()).await.unwrap();
    }
}
