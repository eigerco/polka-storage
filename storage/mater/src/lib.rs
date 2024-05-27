#![warn(unused_crate_dependencies)]

mod multicodec;
mod unixfs;
mod v1;
mod v2;

use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::Cursor,
};

use bytes::Bytes;
use integer_encoding::VarInt;
use ipld_core::codec::Codec;
use multicodec::SHA_256_CODE;
use serde_ipld_dagcbor::codec::DagCborCodec;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWrite;
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
    /// CAR files MUST have **one or more** [`Cid`] roots.
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

    /// See [`ProtobufError`](quick_protobuf::Error) for more information.
    #[error(transparent)]
    ProtobufError(#[from] quick_protobuf::Error),

    /// See [`DagPbError`](ipld_dagpb::Error) for more information.
    #[error(transparent)]
    DagPbError(#[from] ipld_dagpb::Error),
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, VecDeque},
        io::Cursor,
    };

    use tokio::fs::File;
    use tokio_stream::StreamExt;
    use tokio_util::io::ReaderStream;

    use crate::{
        multicodec::SHA_256_CODE, unixfs::stream_balanced_tree, v2::test_utils::assert_buffer_eq,
        Blockstore, CarV1Header, CarV2Header, CarV2Writer, Index, IndexEntry,
    };

    #[tokio::test]
    async fn file_roundtrip_lorem() {
        let file = File::open("tests/fixtures/original/lorem.txt")
            .await
            .unwrap();
        let chunks = ReaderStream::with_capacity(file, 1024 * 256);

        let tree = stream_balanced_tree(chunks, 174);
        tokio::pin!(tree);

        let mut store = Blockstore::new();
        while let Some(block) = tree.next().await {
            let block = block.unwrap();
            store.push(block.cid, block.data, true);
            // in this particular case, this is ok because we know it's the first and last
            store.set_root(block.cid);
        }
        assert_eq!(store.n_blocks(), 1);

        let mut result_buffer = vec![];
        store.write(&mut result_buffer).await.unwrap();

        let car_contents = tokio::fs::read("tests/fixtures/car_v2/lorem.car")
            .await
            .unwrap();

        assert_buffer_eq(&result_buffer, &car_contents);
    }

    #[tokio::test]
    async fn file_roundtrip_spaceglenda() {
        let file = File::open("tests/fixtures/original/spaceglenda.jpg")
            .await
            .unwrap();
        let chunks = ReaderStream::with_capacity(file, 1024 * 256);

        let tree = stream_balanced_tree(chunks, 174);
        tokio::pin!(tree);

        let mut store = Blockstore::new();
        while let Some(block) = tree.next().await {
            let block = block.unwrap();
            store.push(block.cid, block.data, true);
            if !block.links.is_empty() {
                // in this particular case, this is ok because we know it's the first and last
                store.set_root(block.cid);
            }
        }
        assert_eq!(store.n_blocks(), 4);

        let mut result_buffer = vec![];
        store.write(&mut result_buffer).await.unwrap();

        tokio::fs::write("test", &result_buffer).await.unwrap();

        let car_contents = tokio::fs::read("tests/fixtures/car_v2/spaceglenda.car")
            .await
            .unwrap();

        assert_buffer_eq(&result_buffer, &car_contents);
    }
}

struct Blockstore {
    root: Option<Cid>,
    blocks: Vec<(Cid, Bytes)>,
    indexed: HashSet<Cid>,
}

impl Blockstore {
    fn new() -> Self {
        Self {
            root: None,
            blocks: vec![],
            indexed: HashSet::new(),
        }
    }

    fn header_v2(&self) -> CarV2Header {
        let data_offset = CarV2Header::size() as u64;
        let data_size: u64 = self
            .blocks
            .iter()
            .map(|(cid, bytes)| {
                let size = (cid.encoded_len() + bytes.len()) as u64;
                let varint_size = size.required_space() as u64;
                size + varint_size
            })
            .sum();

        // The size of the [`Header`] when encoded using [`DagCborCodec`].
        //
        // The formula is: `overhead + 37 * roots.len()`.
        // It is based on reversing the CBOR encoding, see an example:
        // ```text
        // A2                                      # map(2)
        //    65                                   # text(5)
        //       726F6F7473                        # "roots"
        //    81                                   # array(1)
        //       D8 2A                             # tag(42)
        //          58 25                          # bytes(37)
        //             00015512206D623B17625E25CBDA46D17AC89C26B3DB63544701E2C0592626320DBEFD515B
        //    67                                   # text(7)
        //       76657273696F6E                    # "version"
        //    01                                   # unsigned(1)
        // ```
        // In this case we're doing a single root, so we just use the fixed size: 58
        // let header_v1_length = 58;
        let header_v1_length = DagCborCodec::encode_to_vec(&self.header_v1())
            .unwrap()
            .len() as u64;
        let header_v1_varint = header_v1_length.required_space() as u64;

        let car_v1_payload_length = header_v1_length + header_v1_varint + data_size;

        let index_offset = data_offset + car_v1_payload_length; // when there's no padding
        CarV2Header::new(false, data_offset, car_v1_payload_length, index_offset)
    }

    fn header_v1(&self) -> Option<CarV1Header> {
        self.root.map(|root| CarV1Header::new(vec![root]))
    }

    fn push(&mut self, cid: Cid, data: Bytes, index: bool) {
        self.blocks.push((cid, data));
        if index {
            self.indexed.insert(cid);
        }
    }

    fn set_root(&mut self, cid: Cid) {
        self.root = Some(cid);
    }

    fn n_blocks(&self) -> usize {
        self.blocks.len()
    }

    async fn write<W>(mut self, writer: W) -> Result<usize, Error>
    where
        W: AsyncWrite + Unpin,
    {
        let mut position = 0;

        let mut writer = CarV2Writer::new(writer);
        let header_v2 = self.header_v2();
        let car_v1_start = writer.write_header(&header_v2).await?;
        position += car_v1_start;
        let header_v1 = self.header_v1().ok_or(Error::EmptyRootsError)?;
        position += writer.write_v1_header(&header_v1).await?;

        let mut offsets = HashMap::new(); // 110
        for (cid, block) in self.blocks.drain(..) {
            if self.indexed.contains(&cid) {
                offsets.insert(cid, position - car_v1_start);
            }
            position += writer.write_block(&cid, &block).await?;
        }
        // 7712

        let index = Index::MultihashIndexSorted(MultihashIndexSorted::from_single_width(
            SHA_256_CODE,
            SingleWidthIndex::new(
                Sha256::output_size() as u32,
                offsets.len() as u64,
                offsets
                    .into_iter()
                    .map(|(cid, offset)| {
                        IndexEntry::new(cid.hash().digest().to_vec(), offset as u64)
                    })
                    .collect(),
            )
            .into(),
        ));
        position += writer.write_index(&index).await?;

        Ok(position)
    }
}
