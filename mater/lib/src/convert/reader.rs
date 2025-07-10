use std::{
    collections::{HashMap, VecDeque},
    io::Cursor,
    path::Path,
};

use async_stream::try_stream;
use futures::{future, Stream, TryStreamExt};
use ipld_core::{cid::Cid, codec::Codec};
use ipld_dagpb::{DagPbCodec, PbNode};
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncSeek, AsyncSeekExt},
};

use crate::{
    multicodec,
    v1::{BlockMetadata, CarReader, CarReaderExt},
    v2::CarReader as _,
    Error,
};

/// Extracts the raw data from a CARv2 file.
/// It expects the CAR file to have only 1 root.
pub struct FileReader<R> {
    reader: R,
    index: HashMap<Cid, BlockMetadata>,
}

impl<R> FileReader<R>
where
    R: AsyncRead + AsyncSeek + Unpin,
{
    /// Creates a new [`FileReader`] from the given reader.
    pub async fn new(reader: R) -> Result<Self, Error>
    where
        R: AsyncRead + AsyncSeek + Unpin,
    {
        let mut self_ = Self {
            reader,
            index: HashMap::with_capacity(1),
        };
        self_.naive_build_index().await?;
        Ok(self_)
    }
}

impl FileReader<File> {
    /// Creates a [`FileReader`] from the given file path.
    pub async fn from_path<P>(path: P) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        Self::new(File::open(path).await?).await
    }
}

impl FileReader<Cursor<Vec<u8>>> {
    /// Creates a [`FileReader`] from a vector of bytes.
    pub async fn from_vec(vec: Vec<u8>) -> Result<Self, Error> {
        Self::new(Cursor::new(vec)).await
    }
}

impl<R> FileReader<R>
where
    R: AsyncRead + AsyncSeek + Unpin,
{
    /// Returns the root of the CAR file.
    ///
    /// Always returns a non-empty vector, if there are no roots the error
    /// `Error::WrongNumberOfRoots` is returned.
    pub async fn roots(&mut self) -> Result<Vec<Cid>, Error> {
        self.reader.rewind().await?;
        self.reader.read_pragma().await?;
        self.reader.read_v2_header().await?;

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
        self.reader.rewind().await?;
        let _ = self.reader.read_pragma().await?;
        let v2_header = self.reader.read_v2_header().await?;
        let _ = self.reader.read_v1_header().await?;

        let data_end = v2_header.data_offset + v2_header.data_size;

        loop {
            let position = self.reader.stream_position().await?;
            if position >= data_end {
                break;
            }

            let block_metadata = self.reader.read_block_metadata().await?;
            self.index.insert(block_metadata.cid, block_metadata);
        }

        Ok(())
    }

    /// Traverse the tree under the given [`Cid`], yielding the data in the leaves.
    fn tree_stream(mut self, cid: Cid) -> impl Stream<Item = Result<(Cid, Vec<u8>), Error>> {
        try_stream! {
            let block_metadata = self.index.get(&cid).ok_or_else(|| Error::MissingCid(cid))?;
            let mut queue = VecDeque::new();
            queue.push_back(block_metadata);

            while let Some(block_metadata) = queue.pop_front() {
                match block_metadata.cid.codec() {
                    multicodec::RAW_CODE => {
                        self.reader
                            .seek(std::io::SeekFrom::Start(block_metadata.block_offset))
                            .await?;
                        let block = self.reader.read_block().await?;
                        yield block;
                    }
                    multicodec::DAG_PB_CODE => {
                        self.reader
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

    /// Returns a stream of the chunks of the tree under the provided [`Cid`].
    ///
    /// To retrieve a full file, use [`FileReader::roots`] to retrieve the root
    /// before using [`chunk_stream`].
    pub fn chunk_stream(self, cid: Cid) -> impl Stream<Item = Result<Vec<u8>, Error>> {
        self.tree_stream(cid)
            .and_then(|(_, chunk)| future::ok(chunk))
    }
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use futures::StreamExt;

    use crate::{test_utils::assert_buffer_eq, FileReader};

    #[tokio::test]
    async fn read_duplicated_blocks() {
        let mut loader = FileReader::from_path("tests/fixtures/car_v2/zero.car")
            .await
            .unwrap();
        let root = loader.roots().await.unwrap()[0];

        let mut buffer = vec![];
        loader
            .chunk_stream(root)
            .for_each(|res| {
                let chunk = res.unwrap();
                buffer.extend(chunk);
                futures::future::ready(())
            })
            .await;

        let expected = [0u8; 524288].as_slice();
        assert_buffer_eq!(expected, buffer.as_slice());
    }

    async fn load_and_compare<P1, P2>(original: P1, path: P2)
    where
        P1: AsRef<Path>,
        P2: AsRef<Path>,
    {
        let original = tokio::fs::read(original).await.unwrap();
        let mut car = FileReader::from_path(path).await.unwrap();

        let root = car.roots().await.unwrap()[0];

        let mut out_buffer = vec![];
        car.chunk_stream(root)
            .for_each(|res| {
                let chunk = res.unwrap();
                out_buffer.extend(chunk);
                futures::future::ready(())
            })
            .await;

        assert_buffer_eq!(original, &out_buffer);
    }

    #[tokio::test]
    async fn read_empty() {
        let mut car = FileReader::from_path("tests/fixtures/car_v2/empty.car")
            .await
            .unwrap();
        let root = car.roots().await.unwrap()[0];

        let mut out_buffer: Vec<u8> = vec![];
        car.chunk_stream(root)
            .for_each(|res| {
                let chunk = res.unwrap();
                out_buffer.extend(chunk);
                futures::future::ready(())
            })
            .await;

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
