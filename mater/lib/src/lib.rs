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
mod cid;
mod multicodec;
mod stores;
mod unixfs;
mod v1;
mod v2;

use std::{
    collections::{HashMap, HashSet},
    io::SeekFrom,
};

pub use ipld_core::cid::Cid;
use ipld_core::codec::Codec;
use ipld_dagpb::DagPbCodec;
pub use multicodec::{DAG_PB_CODE, IDENTITY_CODE, RAW_CODE};
pub use stores::{
    create_filestore, Blockstore, Config, FileBlockstore, DEFAULT_CHUNK_SIZE, DEFAULT_TREE_WIDTH,
};
use tokio::io::{AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWriteExt};
pub use v1::{Header as CarV1Header, Reader as CarV1Reader, Writer as CarV1Writer};
pub use v2::{
    verify_cid, Characteristics, Header as CarV2Header, Index, IndexEntry, IndexSorted,
    MultihashIndexSorted, Reader as CarV2Reader, SingleWidthIndex, Writer as CarV2Writer,
};

/// Represents the location and size of a block in the CAR file.
pub struct BlockLocation {
    /// The byte offset in the CAR file where the block starts.
    pub offset: u64,
    /// The size (in bytes) of the block.
    pub size: u64,
}

/// A simple blockstore backed by a CAR file and its index.
pub struct CarBlockStore<R> {
    reader: R,
    /// Mapping from CID to block location.
    pub index: HashMap<Cid, BlockLocation>,
}

impl<R> CarBlockStore<R>
where
    R: AsyncSeekExt + AsyncReadExt + Unpin,
{
    /// Extract content by traversing the UnixFS DAG using the index.
    pub async fn extract_content_via_index<W>(
        &mut self,
        root: &Cid,
        output: &mut W,
    ) -> Result<(), Error>
    where
        W: AsyncWriteExt + Unpin,
    {
        // To avoid processing a block more than once.
        let mut processed = HashSet::new();
        // We use a stack for DFS traversal.
        let mut to_process = vec![root.clone()];

        while let Some(current_cid) = to_process.pop() {
            if processed.contains(&current_cid) {
                continue;
            }
            processed.insert(current_cid.clone());

            // Retrieve block by CID via the index.
            let block_bytes = self.get_block(&current_cid).await?;

            // Write the raw block data. In a real UnixFS traversal you might need
            // to reconstruct file content in order.
            output.write_all(&block_bytes).await?;

            // If the block is a DAG-PB node, decode and enqueue its children.
            if current_cid.codec() == crate::multicodec::DAG_PB_CODE {
                let mut cursor = std::io::Cursor::new(&block_bytes);
                // Propagate any error that occurs during decoding.
                let pb_node: ipld_dagpb::PbNode =
                    DagPbCodec::decode(&mut cursor).map_err(Error::DagPbError)?;
                for link in pb_node.links {
                    if !processed.contains(&link.cid) {
                        to_process.push(link.cid);
                    }
                }
            }
        }

        Ok(())
    }
}

impl<R> CarBlockStore<R>
where
    R: AsyncSeek + AsyncReadExt + Unpin,
{
    /// Given a reader positioned at the start of a CAR file,
    /// load the CARv2 index and build a mapping of CID -> (offset, size).
    /// For simplicity, assume the CAR header has been read and the index offset is known.
    pub async fn load_index(
        mut reader: R,
        index_offset: u64,
    ) -> Result<HashMap<Cid, BlockLocation>, Error> {
        // Seek to the start of the index.
        reader.seek(SeekFrom::Start(index_offset)).await?;
        // Parse the index according to the CARv2 spec. For demonstration,
        // we assume a very simple format where each index entry is:
        //   [CID length (u8)][CID bytes][offset (u64)][size (u64)]
        let mut index = HashMap::new();
        // In a real implementation you’d read until EOF or index length.
        // Here we use a simple loop:
        loop {
            let cid_len = match reader.read_u8().await {
                Ok(n) => n as usize,
                Err(_) => break,
            };
            let mut cid_buf = vec![0u8; cid_len];
            reader.read_exact(&mut cid_buf).await?;
            let cid = Cid::try_from(cid_buf).map_err(|e| Error::Other(e.to_string()))?;

            let offset = reader.read_u64_le().await?;
            let size = reader.read_u64_le().await?;
            index.insert(cid, BlockLocation { offset, size });
        }
        Ok(index)
    }

    /// Retrieve a block by its CID. This method uses the in-memory index
    /// to seek directly to the block’s location.
    pub async fn get_block(&mut self, cid: &Cid) -> Result<Vec<u8>, Error> {
        if let Some(location) = self.index.get(cid) {
            self.reader.seek(SeekFrom::Start(location.offset)).await?;
            let mut buf = vec![0u8; location.size as usize];
            self.reader.read_exact(&mut buf).await?;
            Ok(buf)
        } else {
            Err(Error::BlockNotFound(cid.to_string()))
        }
    }
}

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

    /// Returned when the number of roots is wrong.
    #[error("Wrong number of roots")]
    WrongNumberOfRoots,

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

    /// Error returned when CID verification fails
    #[error("CID is not as expected")]
    InvalidCid,

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

    /// Catch-all error for miscellaneous cases.
    #[error("other error: {0}")]
    Other(String),

    /// Error indicating that the requested block could not be found found in the CAR file's index.
    #[error("block not found: {0}")]
    BlockNotFound(String),
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
