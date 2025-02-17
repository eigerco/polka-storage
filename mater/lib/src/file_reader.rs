use std::{
    collections::{HashMap, VecDeque},
    io::Cursor,
    ops::Deref,
    path::Path,
};

use async_stream::try_stream;
use futures::{Stream, TryStreamExt};
use ipld_core::{cid::Cid, codec::Codec};
use ipld_dagpb::{DagPbCodec, PbNode};
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncSeek, AsyncSeekExt, AsyncWriteExt},
};

use crate::{multicodec, v1::BlockMetadata, v2, Error};

/// Extracts the raw data from a CARv2 file.
/// It expects the CAR file to have only 1 root.
pub struct CarExtractor<R>
where
    R: AsyncRead + AsyncSeek + Unpin,
{
    reader: v2::Reader<R>,
    index: HashMap<Cid, PartialNode>,
}

impl CarExtractor<File> {
    /// Creates a [`FileLoader`] from the given file path.
    pub async fn from_path<P>(path: P) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        let file = File::open(path).await?;
        let mut loader = Self {
            reader: v2::Reader::new(file),
            index: HashMap::new(),
        };
        loader.naive_build_index().await?;
        Ok(loader)
    }
}

impl CarExtractor<Cursor<Vec<u8>>> {
    /// Creates a [`CarExtractor`] from a vector of bytes.
    pub async fn from_vec(vec: Vec<u8>) -> Result<Self, Error> {
        let mut loader = Self {
            reader: v2::Reader::new(Cursor::new(vec)),
            index: HashMap::new(),
        };
        loader.naive_build_index().await?;
        Ok(loader)
    }
}

impl<R> CarExtractor<R>
where
    R: AsyncRead + AsyncSeek + Unpin,
{
    /// Returns the root of the CAR file.
    ///
    /// Always returns a non-empty vector, if there are no roots the error
    /// `Error::WrongNumberOfRoots` is returned.
    pub async fn root(&mut self) -> Result<Vec<Cid>, Error> {
        self.reader.get_inner_mut().rewind().await?;
        self.reader.read_pragma().await?;
        self.reader.read_header().await?;

        let roots = self.reader.read_v1_header().await?.roots;
        if roots.is_empty() {
            return Err(Error::WrongNumberOfRoots);
        }
        Ok(roots)
    }

    /// Indexes the file.
    ///
    /// *Always* seeks to the start of the file before proceeding with indexing.
    /// This function performs indexing *naively*, it will sequentially read all block "headers"
    /// (Cid, data start offset and data length) without loading the blocks into memory.
    async fn naive_build_index(&mut self) -> Result<(), Error> {
        // Indexing must always be made from the start
        self.reader.get_inner_mut().rewind().await?;
        let _ = self.reader.read_pragma().await?;
        let v2_header = self.reader.read_header().await?;
        let _ = self.reader.read_v1_header().await?;

        let data_end = v2_header.data_offset + v2_header.data_size;

        loop {
            let position = self.reader.get_inner_mut().stream_position().await?;
            if position >= data_end {
                break;
            }

            let block_metadata = self.reader.read_block_metadata().await?;
            self.index
                .insert(block_metadata.cid, block_metadata.try_into()?);
        }

        Ok(())
    }

    /// Traverse the tree under the given [`Cid`], yielding the data in the leaves.
    fn tree_stream<'a>(
        &'a mut self,
        cid: &'a Cid,
    ) -> impl Stream<Item = Result<(Cid, Vec<u8>), Error>> + 'a {
        try_stream! {
            let partial_node = self.index.get(&cid).ok_or_else(|| Error::MissingCid(*cid))?;
            let mut queue = VecDeque::new();
            queue.push_back(partial_node);

            while let Some(partial_node) = queue.pop_front() {
                match partial_node {
                    PartialNode::Leaf(metadata) => {
                        self.reader
                            .get_inner_mut()
                            .seek(std::io::SeekFrom::Start(metadata.block_offset))
                            .await?;
                        let block = self.reader.read_block().await?;
                        yield block;
                    }
                    PartialNode::Stem(metadata) => {
                        self.reader
                            .get_inner_mut()
                            .seek(std::io::SeekFrom::Start(metadata.block_offset))
                            .await?;
                        let (_, block) = self.reader.read_block().await?;

                        let pb_node: PbNode = DagPbCodec::decode_from_slice(block.as_slice())?;
                        for link in pb_node.links {
                            let partial_node = self.index.get(&link.cid).ok_or_else(|| Error::MissingCid(link.cid))?;
                            queue.push_back(partial_node);
                        }
                    }
                }
            }
        }
    }

    /// Writes the content tree for the given [`Cid`] into `w`.
    ///
    /// This is equivalent to reading the stream of blocks from [`Self::load_cid`] into a writer.
    async fn copy_tree<W>(&mut self, cid: &Cid, mut w: W) -> Result<(), Error>
    where
        W: AsyncWriteExt + Unpin,
    {
        let loader = self.tree_stream(cid);
        tokio::pin!(loader);
        while let Some((_, block)) = loader.try_next().await? {
            w.write_all(block.as_slice()).await?;
        }
        w.flush().await?;
        Ok(())
    }

    /// Writes the content tree from the root into `w`.
    ///
    /// Assumes the file at least one root and will copy the file under the first root.
    pub async fn copy_to_writer<W>(&mut self, mut writer: W) -> Result<(), Error>
    where
        W: AsyncWriteExt + Unpin,
    {
        let root = self.root().await?[0];
        self.copy_tree(&root, &mut writer).await
    }
}

/// Partial "re-implementation" of [`unixfs::TreeNode`].
enum PartialNode {
    Leaf(BlockMetadata),
    Stem(BlockMetadata),
}

impl Deref for PartialNode {
    type Target = BlockMetadata;

    fn deref(&self) -> &Self::Target {
        match self {
            PartialNode::Leaf(b) => b,
            PartialNode::Stem(b) => b,
        }
    }
}

impl TryFrom<BlockMetadata> for PartialNode {
    type Error = Error;

    fn try_from(value: BlockMetadata) -> Result<Self, Self::Error> {
        match value.cid.codec() {
            multicodec::RAW_CODE => Ok(Self::Leaf(value)),
            multicodec::DAG_PB_CODE => Ok(Self::Stem(value)),
            unknown_codec => Err(Error::UnknownCidCodec(unknown_codec)),
        }
    }
}

#[cfg(test)]
mod test {
    use std::io::Cursor;

    use crate::{Blockstore, CarExtractor};

    /// Ensures that duplicated blocks
    #[tokio::test]
    async fn read_duplicated_blocks() {
        let raw_input = std::iter::repeat(0).take(4096).collect::<Vec<u8>>();

        let mut bs = Blockstore::with_parameters(Some(1024), None);
        bs.read(Cursor::new(raw_input.clone())).await.unwrap();

        // 1519 is the expected CAR file size after deduplicating and writing the CAR file
        let mut out_car_buffer = Vec::with_capacity(1519);
        bs.write(&mut out_car_buffer).await.unwrap();
        assert_eq!(out_car_buffer.len(), 1519);

        let mut loader = CarExtractor::from_vec(out_car_buffer).await.unwrap();
        let root = loader.root().await.unwrap();

        let mut out_check = Cursor::new(vec![1u8; 4096]);
        loader.copy_tree(&root, &mut out_check).await.unwrap();

        assert_eq!(raw_input, out_check.into_inner());
    }

    #[tokio::test]
    async fn read_wrapped() {
        let spaceglenda_original = tokio::fs::read("tests/fixtures/original/spaceglenda.jpg")
            .await
            .unwrap();
        let mut spaceglenda_wrapped_loader =
            CarExtractor::from_path("tests/fixtures/car_v2/spaceglenda_wrapped.car")
                .await
                .unwrap();

        let mut out_buffer: Vec<u8> = vec![];
        let root = spaceglenda_wrapped_loader.root().await.unwrap();
        spaceglenda_wrapped_loader
            .copy_tree(&root, &mut out_buffer)
            .await
            .unwrap();

        crate::test_utils::assert_buffer_eq!(spaceglenda_original, &out_buffer);
    }
}
