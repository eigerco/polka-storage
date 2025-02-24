use std::{
    collections::{HashMap, VecDeque},
    io::Cursor,
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
    index: HashMap<Cid, BlockMetadata>,
}

impl CarExtractor<File> {
    /// Creates a [`CarExtractor`] from the given file path.
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
    pub async fn roots(&mut self) -> Result<Vec<Cid>, Error> {
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
            self.index.insert(block_metadata.cid, block_metadata);
        }

        Ok(())
    }

    /// Traverse the tree under the given [`Cid`], yielding the data in the leaves.
    fn tree_stream<'a>(
        &'a mut self,
        cid: &'a Cid,
    ) -> impl Stream<Item = Result<(Cid, Vec<u8>), Error>> + 'a {
        try_stream! {
            let block_metadata = self.index.get(&cid).ok_or_else(|| Error::MissingCid(*cid))?;
            let mut queue = VecDeque::new();
            queue.push_back(block_metadata);

            while let Some(block_metadata) = queue.pop_front() {
                match block_metadata.cid.codec() {
                    multicodec::RAW_CODE => {
                        self.reader
                            .get_inner_mut()
                            .seek(std::io::SeekFrom::Start(block_metadata.block_offset))
                            .await?;
                        let block = self.reader.read_block().await?;
                        yield block;
                    }
                    multicodec::DAG_PB_CODE => {
                        self.reader
                            .get_inner_mut()
                            .seek(std::io::SeekFrom::Start(block_metadata.block_offset))
                            .await?;
                        let (_, block) = self.reader.read_block().await?;

                        let pb_node: PbNode = DagPbCodec::decode_from_slice(block.as_slice())?;
                        for link in pb_node.links {
                            let block_metadata = self.index.get(&link.cid).ok_or_else(|| Error::MissingCid(link.cid))?;
                            queue.push_back(block_metadata);
                        }
                    }
                    unknown_codec => {
                        // return doesn't work here
                        Err(Error::UnknownCidCodec(unknown_codec))?;
                    },
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
        let root = self.roots().await?[0];
        self.copy_tree(&root, &mut writer).await
    }
}

#[cfg(test)]
mod test {
    use std::{io::Cursor, path::Path};

    use crate::CarExtractor;

    #[tokio::test]
    async fn read_duplicated_blocks() {
        let mut loader = CarExtractor::from_path("tests/fixtures/car_v2/zero.car")
            .await
            .unwrap();
        let root = loader.roots().await.unwrap()[0];
        let mut out_check = Cursor::new(vec![1u8; 4096]);
        loader.copy_tree(&root, &mut out_check).await.unwrap();

        let expected = [0u8; 524288].as_slice();
        let inner = out_check.into_inner();
        let result = inner.as_slice();

        assert_eq!(expected, result);
    }

    async fn load_and_compare<P1, P2>(original: P1, path: P2)
    where
        P1: AsRef<Path>,
        P2: AsRef<Path>,
    {
        let original = tokio::fs::read(original).await.unwrap();
        let mut car = CarExtractor::from_path(path).await.unwrap();

        let mut out_buffer: Vec<u8> = vec![];
        let root = car.roots().await.unwrap()[0];
        car.copy_tree(&root, &mut out_buffer).await.unwrap();

        crate::test_utils::assert_buffer_eq!(original, &out_buffer);
    }

    #[tokio::test]
    async fn read_empty() {
        let mut car = CarExtractor::from_path("tests/fixtures/car_v2/empty.car")
            .await
            .unwrap();
        let mut out_buffer: Vec<u8> = vec![];
        let root = car.roots().await.unwrap()[0];
        car.copy_tree(&root, &mut out_buffer).await.unwrap();
        assert!(out_buffer.is_empty());
    }

    #[tokio::test]
    async fn read_lorem() {
        load_and_compare(
            "tests/fixtures/original/lorem.txt",
            "tests/fixtures/car_v2/lorem.car",
        )
        .await
    }

    #[tokio::test]
    async fn read_spaceglenda() {
        load_and_compare(
            "tests/fixtures/original/spaceglenda.jpg",
            "tests/fixtures/car_v2/spaceglenda.car",
        )
        .await
    }

    #[tokio::test]
    async fn read_spaceglenda_wrapped() {
        load_and_compare(
            "tests/fixtures/original/spaceglenda.jpg",
            "tests/fixtures/car_v2/spaceglenda_wrapped.car",
        )
        .await
    }
}
