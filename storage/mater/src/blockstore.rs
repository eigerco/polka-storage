use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use bytes::Bytes;
use integer_encoding::VarInt;
use ipld_core::cid::Cid;
use sha2::{Digest, Sha256};
use tokio::{fs::File, io::AsyncWrite};
use tokio_stream::StreamExt;
use tokio_util::io::ReaderStream;

use crate::{
    multicodec::SHA_256_CODE, unixfs::stream_balanced_tree, CarV1Header, CarV2Header, CarV2Writer,
    Error, Index, IndexEntry, MultihashIndexSorted, SingleWidthIndex,
};

// https://github.com/ipfs/boxo/blob/f4fe8997dcbeb39b3a4842d8f08b34739bfd84a4/chunker/parse.go#L13
const DEFAULT_CHUNK_SIZE: usize = 1024 * 256;
const DEFAULT_TREE_WIDTH: usize = 174;

pub struct Blockstore {
    root: Option<Cid>,
    blocks: Vec<(Cid, Bytes)>,
    indexed: HashSet<Cid>,

    chunk_size: usize,
    tree_width: usize,
}

impl Blockstore {
    pub fn new() -> Self {
        Self {
            root: None,
            blocks: vec![],
            indexed: HashSet::new(),
            chunk_size: DEFAULT_CHUNK_SIZE,
            tree_width: DEFAULT_TREE_WIDTH,
        }
    }

    pub async fn from_file<P>(path: P) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        let mut store = Blockstore::new();

        let file = File::open(path).await?;
        let chunks = ReaderStream::with_capacity(file, store.chunk_size);

        let tree = stream_balanced_tree(chunks, store.tree_width);
        tokio::pin!(tree);

        while let Some(block) = tree.next().await {
            let block = block.unwrap();
            store.push(block.cid, block.data, true);
            // in this particular case, this is ok because we know it's the first and last
            store.set_root(block.cid);
        }

        Ok(store)
    }

    pub async fn write<W>(mut self, writer: W) -> Result<usize, Error>
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

        let mut offsets = HashMap::new();
        for (cid, block) in self.blocks.drain(..) {
            if self.indexed.contains(&cid) {
                offsets.insert(cid, position - car_v1_start);
            }
            position += writer.write_block(&cid, &block).await?;
        }

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
        // In this case we're always doing a single root, so we just use the fixed size: 58
        let header_v1_length = 58;
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
}

#[cfg(test)]
mod tests {
    use crate::blockstore::Blockstore;
    use crate::test_utils::assert_buffer_eq;

    #[tokio::test]
    async fn file_roundtrip_lorem() {
        let store = Blockstore::from_file("tests/fixtures/original/lorem.txt")
            .await
            .unwrap();

        let mut result_buffer = vec![];
        store.write(&mut result_buffer).await.unwrap();

        let car_contents = tokio::fs::read("tests/fixtures/car_v2/lorem.car")
            .await
            .unwrap();

        assert_buffer_eq!(&result_buffer, &car_contents);
    }

    #[tokio::test]
    async fn file_roundtrip_spaceglenda() {
        let store = Blockstore::from_file("tests/fixtures/original/spaceglenda.jpg")
            .await
            .unwrap();

        assert_eq!(store.n_blocks(), 4);

        let mut result_buffer = vec![];
        store.write(&mut result_buffer).await.unwrap();

        tokio::fs::write("test", &result_buffer).await.unwrap();

        let car_contents = tokio::fs::read("tests/fixtures/car_v2/spaceglenda.car")
            .await
            .unwrap();

        assert_buffer_eq!(&result_buffer, &car_contents);
    }
}
