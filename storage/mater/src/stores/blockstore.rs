// NOTE(@jmg-duarte,28/05/2024): the blockstore can (and should) evolve to support other backends.
// At the time of writing, there is no need invest more time in it because the current PR(#25) is delayed enough.

use std::collections::{HashMap, HashSet};

use bytes::Bytes;
use indexmap::IndexMap;
use integer_encoding::VarInt;
use ipld_core::cid::Cid;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_stream::StreamExt;
use tokio_util::io::ReaderStream;

use super::{DEFAULT_BLOCK_SIZE, DEFAULT_TREE_WIDTH};
use crate::{
    multicodec::SHA_256_CODE, unixfs::stream_balanced_tree, CarV1Header, CarV2Header, CarV2Writer,
    Error, Index, IndexEntry, MultihashIndexSorted, SingleWidthIndex,
};

/// The [`Blockstore`] stores pairs of [`Cid`] and [`Bytes`] in memory.
///
/// The store will chunk data blocks into `chunk_size` and "gather" nodes in groups with at most `tree_width` children.
/// You can visualize the underlying tree in <https://dag.ipfs.tech/>, using the "Balanced DAG" layout.
///
/// It is necessary to keep the blocks somewhere before writing them to a file since the CARv2 header
/// has data size, index offset and indexes fields, all these requiring information that only becomes
/// "available" after you process all the blocks.
///
/// The store keeps track of ([`Cid`], [`Bytes`]) pairs, performing de-duplication based on the [`Cid`].
///
/// **Important note: currently, the blockstore only supports a single file!**
pub struct Blockstore {
    root: Option<Cid>,
    blocks: IndexMap<Cid, Bytes>,
    indexed: HashSet<Cid>,

    chunk_size: usize,
    tree_width: usize,
}

impl Blockstore {
    /// The size of the [`Header`] when encoded using [`DagCborCodec`].
    ///
    /// The formula is: `overhead + 37 * roots.len()`.
    /// It is based on reversing the CBOR encoding, see an example:
    /// ```text
    /// A2                                      # map(2)
    ///    65                                   # text(5)
    ///       726F6F7473                        # "roots"
    ///    81                                   # array(1)
    ///       D8 2A                             # tag(42)
    ///          58 25                          # bytes(37)
    ///             00015512206D623B17625E25CBDA46D17AC89C26B3DB63544701E2C0592626320DBEFD515B
    ///    67                                   # text(7)
    ///       76657273696F6E                    # "version"
    ///    01                                   # unsigned(1)
    /// ```
    /// In this case we're always doing a single root, so we just use the fixed size: 58
    ///
    /// Is this cheating? Yes. The alternative is to encode the CARv1 header twice.
    /// We can cache it, but for now, this should be better.
    const V1_HEADER_OVERHEAD: u64 = 58;

    /// Construct a new [`Blockstore`], using the default parameters.
    pub fn new() -> Self {
        Default::default()
    }

    /// Construct a new [`Blockstore`], using custom parameters.
    /// If set to `None`, the corresponding default value will be used.
    pub fn with_parameters(chunk_size: Option<usize>, tree_width: Option<usize>) -> Self {
        // NOTE(@jmg-duarte,28/05/2024): once the time comes, this method should probably be replaced with a builder
        Self {
            root: None,
            blocks: IndexMap::new(),
            indexed: HashSet::new(),
            chunk_size: chunk_size.unwrap_or(DEFAULT_BLOCK_SIZE),
            tree_width: tree_width.unwrap_or(DEFAULT_TREE_WIDTH),
        }
    }

    /// Fully read the contents of an arbitrary `reader` into the [`Blockstore`],
    /// converting the contents into a CARv2 file.
    pub async fn read<R>(&mut self, reader: R) -> Result<(), Error>
    where
        R: AsyncRead + Unpin + Send,
    {
        let chunks = ReaderStream::with_capacity(reader, self.chunk_size);

        // The `stream -> pin -> peekable` combo instead of `stream -> peekable -> pin` feels weird
        // but it has to do with two things:
        // - The fact that the stream can be self-referential:
        //   https://users.rust-lang.org/t/why-is-pin-mut-needed-for-iteration-of-async-stream/51107
        // - Using a tokio_stream::Peekable instead of futures::Peekable, they differ on who is required to be pinned
        //  - tokio_stream::Peekable::peek(&mut self)
        //    https://github.com/tokio-rs/tokio/blob/14c17fc09656a30230177b600bacceb9db33e942/tokio-stream/src/stream_ext/peekable.rs#L26-L37
        //  - futures::Peekable::peek(self: Pin<&mut Self>)
        //    https://github.com/rust-lang/futures-rs/blob/c507ff833728e2979cf5519fc931ea97308ec876/futures-util/src/stream/stream/peek.rs#L38-L40
        let tree = stream_balanced_tree(chunks, self.tree_width);
        tokio::pin!(tree);
        let mut tree = tree.peekable();

        while let Some(block) = tree.next().await {
            let (cid, bytes) = block?;
            self.insert(cid, bytes, true);

            // If the stream is exhausted, we know the current block is the root
            if tree.peek().await.is_none() {
                // The root should always be indexed, there's no official spec saying it should though, it just makes sense.
                // So, if the insert line is changed, the root should be placed in the `indexed` structure here
                self.root = Some(cid);
            }
        }

        Ok(())
    }

    /// Write the contents of the [`Blockstore`] as CARv2 to a writer.
    pub async fn write<W>(mut self, writer: W) -> Result<usize, Error>
    where
        W: AsyncWrite + Unpin,
    {
        let mut position = 0;

        let mut writer = CarV2Writer::new(writer);
        let header_v2 = self.header_v2();

        // Writing the CARv1 starts where the CARv2 header ends
        // this value is required for indexing,
        // whose offset starts at the beginning of the CARv1 header
        let car_v1_start = writer.write_header(&header_v2).await?;
        position += car_v1_start;

        // CARv1 files are REQUIRED to have a root
        let header_v1 = self
            .root
            .map(|root| CarV1Header::new(vec![root]))
            .ok_or(Error::EmptyRootsError)?;
        position += writer.write_v1_header(&header_v1).await?;

        let mut offsets = HashMap::new();
        for (cid, block) in self.blocks.drain(..) {
            if self.indexed.contains(&cid) {
                offsets.insert(cid, position - car_v1_start);
            }
            position += writer.write_block(&cid, &block).await?;
        }

        let count = offsets.len() as u64;
        let entries = offsets
            .into_iter()
            .map(|(cid, offset)| IndexEntry::new(cid.hash().digest().to_vec(), offset as u64))
            .collect();
        let index = Index::MultihashIndexSorted(MultihashIndexSorted::from_single_width(
            SHA_256_CODE,
            SingleWidthIndex::new(Sha256::output_size() as u32, count, entries).into(),
        ));
        position += writer.write_index(&index).await?;

        Ok(position)
    }

    /// Get the [`CarV2Header`] that will be written out.
    fn header_v2(&self) -> CarV2Header {
        let data_offset = CarV2Header::SIZE as u64;
        let data_size: u64 = self
            .blocks
            .iter()
            .map(|(cid, bytes)| {
                let size = (cid.encoded_len() + bytes.len()) as u64;
                let varint_size = size.required_space() as u64;
                size + varint_size
            })
            .sum();

        let header_v1_varint = Self::V1_HEADER_OVERHEAD.required_space() as u64;
        let car_v1_payload_length = Self::V1_HEADER_OVERHEAD + header_v1_varint + data_size;

        // If there is padding, this does not apply, however, the go-car tool doesn't seem to ever add padding
        let index_offset = data_offset + car_v1_payload_length;

        // NOTE(@jmg-duarte,28/05/2024): the `fully_indexed` field is currently set to `false` as the
        // go-car tool doesn't seem to ever set it, however, according to the written definition we have from the spec
        // we're performing full indexing, as all blocks are inserted with `index: true`.
        CarV2Header::new(false, data_offset, car_v1_payload_length, index_offset)
    }

    /// Insert a new block into the [`Blockstore`].
    ///
    /// If the [`Cid`] has been previously inserted, this function is a no-op.
    fn insert(&mut self, cid: Cid, data: Bytes, index: bool) {
        if !self.blocks.contains_key(&cid) {
            self.blocks.insert_full(cid, data);
            if index {
                self.indexed.insert(cid);
            }
        }
    }
}

impl Default for Blockstore {
    fn default() -> Self {
        Self {
            root: None,
            blocks: IndexMap::new(),
            indexed: HashSet::new(),
            chunk_size: DEFAULT_BLOCK_SIZE,
            tree_width: DEFAULT_TREE_WIDTH,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{io::Cursor, str::FromStr};

    use ipld_core::{cid::Cid, codec::Codec};
    use ipld_dagpb::{DagPbCodec, PbNode};
    use sha2::{Digest, Sha256};
    use tokio::fs::File;

    use crate::{
        multicodec::{generate_multihash, RAW_CODE, SHA_256_CODE},
        stores::blockstore::Blockstore,
        test_utils::assert_buffer_eq,
        CarV2Header, CarV2Reader, Index,
    };

    #[tokio::test]
    async fn byte_eq_lorem() {
        let file = File::open("tests/fixtures/original/lorem.txt")
            .await
            .unwrap();
        let mut store = Blockstore::new();
        store.read(file).await.unwrap();
        assert_eq!(store.blocks.len(), 1);

        let mut result_buffer = vec![];
        store.write(&mut result_buffer).await.unwrap();

        let car_contents = tokio::fs::read("tests/fixtures/car_v2/lorem.car")
            .await
            .unwrap();

        assert_buffer_eq!(&result_buffer, &car_contents);
    }

    #[tokio::test]
    async fn byte_eq_spaceglenda() {
        let file = File::open("tests/fixtures/original/spaceglenda.jpg")
            .await
            .unwrap();
        let mut store = Blockstore::new();
        store.read(file).await.unwrap();
        assert_eq!(store.blocks.len(), 4);

        let mut result_buffer = vec![];
        store.write(&mut result_buffer).await.unwrap();

        let car_contents = tokio::fs::read("tests/fixtures/car_v2/spaceglenda.car")
            .await
            .unwrap();

        assert_buffer_eq!(&result_buffer, &car_contents);
    }

    #[tokio::test]
    async fn dedup_lorem() {
        let file = File::open("tests/fixtures/original/lorem_4096_dup.txt")
            .await
            .unwrap();
        let mut store = Blockstore::with_parameters(Some(1024), None);
        store.read(file).await.unwrap();
        // We're expecting there to exist a single data block and a root
        assert_eq!(store.blocks.len(), 2);
    }

    // We can't fully validate this test using go-car because they don't offer parametrization
    #[tokio::test]
    async fn dedup_lorem_roundtrip() {
        let file = File::open("tests/fixtures/original/lorem_4096_dup.txt")
            .await
            .unwrap();
        let mut store = Blockstore::with_parameters(Some(1024), None);
        store.read(file).await.unwrap();
        // We're expecting there to exist a single data block and a root
        assert_eq!(store.blocks.len(), 2);

        let mut result_buffer = vec![];
        store.write(&mut result_buffer).await.unwrap();

        let mut cursor = Cursor::new(result_buffer);
        std::io::Seek::rewind(&mut cursor).unwrap();
        let mut car_reader = CarV2Reader::new(cursor);

        car_reader.read_pragma().await.unwrap();

        let car_v2_header = car_reader.read_header().await.unwrap();
        assert_eq!(car_v2_header.data_offset, CarV2Header::SIZE as u64);
        // Extracted with go-car and validated with an hex viewer
        // to extract the values, run the following commands:
        // $ car inspect <output of this process>
        // The dump is necessary because go-car does not support parametrization
        assert_eq!(car_v2_header.data_size, 1358);
        assert_eq!(
            car_v2_header.index_offset,
            (CarV2Header::SIZE as u64) + 1358
        );

        let car_v1_header = car_reader.read_v1_header().await.unwrap();
        assert_eq!(car_v1_header.roots.len(), 1);

        // Extracted with go-car
        let root_cid =
            Cid::from_str("bafybeiapxsorxw7yqywquebgmlz37nyjt44vxlskhx6wcgirkurojow7xu").unwrap();
        assert_eq!(car_v1_header.roots[0], root_cid);

        let original_1024 = tokio::fs::read("tests/fixtures/original/lorem_1024.txt")
            .await
            .unwrap();
        let (cid, data) = car_reader.read_block().await.unwrap();
        assert_buffer_eq!(&data, &original_1024);
        let lorem_cid = Cid::new_v1(RAW_CODE, generate_multihash::<Sha256, _>(&original_1024));
        assert_eq!(cid, lorem_cid);

        let (cid, data) = car_reader.read_block().await.unwrap();
        let node: PbNode = DagPbCodec::decode_from_slice(&data).unwrap();
        assert_eq!(cid, root_cid);

        // There are 4 blocks of repeated 1024 bytes
        assert_eq!(node.links.len(), 4);

        for pb_link in node.links {
            assert_eq!(pb_link.cid, lorem_cid);
            assert_eq!(pb_link.name, Some("".to_string()));
            assert_eq!(pb_link.size, Some(1024));
        }

        let index = car_reader.read_index().await.unwrap();

        match index {
            Index::MultihashIndexSorted(index) => {
                // There's only Sha256
                assert_eq!(index.0.len(), 1);

                let index_sorted = &index.0[&SHA_256_CODE];
                // There's only a single length
                assert_eq!(index_sorted.0.len(), 1);

                let single_width_index = &index_sorted.0[0];
                assert_eq!(single_width_index.count, 2);
                // Sha256 output size (32) + the offset size (8)
                assert_eq!(single_width_index.width, Sha256::output_size() as u32 + 8);
                assert_eq!(single_width_index.entries.len(), 2);

                // Sorting order is byte-wise, I extracted it manually
                assert_eq!(single_width_index.entries[0].offset, 1121);
                assert_eq!(single_width_index.entries[1].offset, 59);
                assert_eq!(
                    single_width_index.entries[0].digest,
                    root_cid.hash().digest()
                );
                assert_eq!(
                    single_width_index.entries[1].digest,
                    lorem_cid.hash().digest()
                );
            }
            Index::IndexSorted(_) => panic!("expected index to be MultihashIndexSorted"),
        }
    }
}
