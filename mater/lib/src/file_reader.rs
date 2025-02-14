use std::{collections::HashMap, ops::Deref, path::Path};

use async_stream::try_stream;
use futures::Stream;
use ipld_core::{cid::Cid, codec::Codec};
use ipld_dagpb::{DagPbCodec, PbNode};
use tokio::{fs::File, io::AsyncSeekExt};

use crate::{multicodec, v1::BlockMetadata, v2, Error};

/// CAR file loader.
pub struct FileLoader {
    reader: v2::Reader<File>,
    index: HashMap<Cid, PartialNode>,
}

impl FileLoader {
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
        loader.index().await?;
        Ok(loader)
    }

    /// Returns the root of the CAR file.
    ///
    /// If the number of roots is not 1, returns [`Error::WrongNumberOfRoots`].
    pub async fn root(&mut self) -> Result<Cid, Error> {
        self.reader.get_inner_mut().rewind().await?;
        self.reader.read_pragma().await?;
        self.reader.read_header().await?;
        // We assume there's a single root
        self.reader
            .read_v1_header()
            .await?
            .roots
            .first()
            .copied()
            .ok_or(Error::WrongNumberOfRoots)
    }

    /// Indexes the file.
    ///
    /// *Always* seeks to the start of the file before proceeding with indexing.
    /// This function performs indexing *naively*, it will sequentially read all block "headers"
    /// (Cid, data start offset and data length) without loading the blocks into memory.
    async fn index(&mut self) -> Result<(), Error> {
        // Indexing must alwasy be made from the start
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
    pub fn load_cid<'a>(
        &'a mut self,
        cid: &'a Cid,
    ) -> impl Stream<Item = Result<(Cid, Vec<u8>), Error>> + 'a {
        try_stream! {
            let partial_node = self.index.get(&cid).ok_or(Error::InvalidCid)?;
            let mut stack = vec![partial_node];

            while let Some(partial_node) = stack.pop() {
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
                            let partial_node = self.index.get(&link.cid).ok_or(Error::InvalidCid)?;
                            stack.push(partial_node);
                        }
                    }
                }
            }
        }
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
            _ => Err(Error::InvalidCid),
        }
    }
}
