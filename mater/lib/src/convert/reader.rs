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

use crate::{
    multicodec,
    v1::{BlockMetadata, CarReader, CarReaderExt},
    v2::CarReader as _,
    Error,
};

/// Extracts the raw data from a CARv2 file.
/// It expects the CAR file to have only 1 root.
///
/// Disambiguation: this writer is not a [`File`](tokio::fs::File) writer,
/// but rather a file writer in the CAR file format sense.
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

    /// Writes the content tree for the given [`Cid`] into `w`.
    pub async fn copy_tree<W>(&mut self, cid: &Cid, mut w: W) -> Result<(), Error>
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

#[cfg(feature = "blockstore")]
pub(crate) mod blockstore {
    use std::{any::type_name, ops::Deref, path::Path};

    use blockstore::Blockstore;
    use futures::TryFutureExt;
    use ipld_core::cid::Cid;
    use tokio::{
        fs::File,
        io::{AsyncRead, AsyncSeek, AsyncSeekExt},
        sync::RwLock,
    };

    use crate::{convert::to_blockstore_cid, v1::CarReader, CidExt, Error, FileReader};

    // Methods in here are marked as unused in the "main" `impl` because they're only used here.
    impl<R> FileReader<R>
    where
        R: AsyncRead + AsyncSeek + Unpin,
    {
        fn has(&self, cid: &Cid) -> bool {
            if cid.get_identity_data().is_some() {
                return true;
            }
            // Since we're using the naive index, if the Cid isn't in the index, there is no Cid inside
            self.index.contains_key(cid)
        }

        async fn get(&mut self, cid: &Cid) -> Result<Option<Vec<u8>>, Error> {
            if let Some(identity_data) = cid.get_identity_data() {
                return Ok(Some(identity_data.to_vec()));
            }

            match self.index.get(&cid) {
                Some(metadata) => {
                    // We could seek directly to the data and not read the Cid, but this is "canonical"
                    self.reader
                        .seek(std::io::SeekFrom::Start(metadata.block_offset))
                        .await?;
                    let (_, block) = self.reader.read_block().await?;
                    Ok(Some(block))
                }
                None => Ok(None),
            }
        }
    }

    /// A read-only [`blockstore::Blockstore`] implementation of [`FileReader`].
    pub struct ReadOnlyBlockstore<R> {
        inner: RwLock<FileReader<R>>,
    }

    impl<R> ReadOnlyBlockstore<R>
    where
        R: AsyncRead + AsyncSeek + Unpin + blockstore::cond_send::CondSync,
    {
        /// Create a new [`CarReadOnlyBlockstore`] from the given reader.
        pub async fn new(reader: R) -> Result<Self, Error> {
            Ok(Self {
                inner: RwLock::new(FileReader::new(reader).await?),
            })
        }
    }

    impl ReadOnlyBlockstore<File> {
        /// Create a new [`CarReadOnlyBlockstore<tokio::io::File>`](CarReadOnlyBlockstore) from the given path.
        pub async fn from_path<P>(path: P) -> Result<Self, Error>
        where
            P: AsRef<Path>,
        {
            Self::new(File::open(path).await?).await
        }
    }

    impl<R> Deref for ReadOnlyBlockstore<R> {
        type Target = RwLock<FileReader<R>>;

        fn deref(&self) -> &Self::Target {
            &self.inner
        }
    }

    impl<R> Blockstore for ReadOnlyBlockstore<R>
    where
        R: AsyncRead + AsyncSeek + Unpin + Send + Sync,
    {
        async fn get<const S: usize>(
            &self,
            cid: &ipld_core::cid::CidGeneric<S>,
        ) -> blockstore::Result<Option<Vec<u8>>> {
            let cid = to_blockstore_cid(cid)?;
            self.inner
                .write()
                .await
                .get(&cid)
                .map_err(|err| blockstore::Error::FatalDatabaseError(err.to_string()))
                .await
        }

        async fn has<const S: usize>(
            &self,
            cid: &ipld_core::cid::CidGeneric<S>,
        ) -> blockstore::Result<bool> {
            let cid = to_blockstore_cid(cid)?;
            Ok(self.inner.read().await.has(&cid))
        }

        async fn put_keyed<const S: usize>(
            &self,
            _: &ipld_core::cid::CidGeneric<S>,
            _: &[u8],
        ) -> blockstore::Result<()> {
            Err(blockstore::Error::FatalDatabaseError(format!(
                "{} is read-only",
                type_name::<Self>()
            )))
        }

        async fn remove<const S: usize>(
            &self,
            _: &ipld_core::cid::CidGeneric<S>,
        ) -> blockstore::Result<()> {
            Err(blockstore::Error::FatalDatabaseError(format!(
                "{} is read-only",
                type_name::<Self>()
            )))
        }

        async fn close(self) -> blockstore::Result<()> {
            Ok(())
        }
    }

    #[cfg(test)]
    mod test {
        use std::{str::FromStr, sync::Arc};

        use ipld_core::cid::{multihash::Multihash, Cid};
        use sha2::Sha256;
        use tokio::fs::File;

        use super::*;
        use crate::{
            multicodec::generate_multihash, test_utils::assert_buffer_eq, IDENTITY_CODE, RAW_CODE,
        };

        type FileBlockstore = ReadOnlyBlockstore<File>;

        #[tokio::test]
        async fn test_identity_cid() {
            let blockstore = FileBlockstore::from_path("tests/fixtures/car_v2/spaceglenda.car")
                .await
                .unwrap();

            let payload = b"Hello World!";
            let multihash = Multihash::wrap(IDENTITY_CODE, payload).unwrap();
            let identity_cid = Cid::new_v1(RAW_CODE, multihash);

            let has_block = blockstore.has(&identity_cid).await.unwrap();
            assert!(has_block);

            let content = blockstore.get(&identity_cid).await.unwrap().unwrap();
            assert_buffer_eq!(&payload, &content);
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
        async fn test_parallel_readers() {
            let blockstore = FileBlockstore::from_path("tests/fixtures/car_v2/spaceglenda.car")
                .await
                .unwrap();
            let blockstore = Arc::new(blockstore);

            // CIDs of the content blocks that the spaceglenda.car contains. We are
            // only looking at the raw content so that our validation is easier later.
            let cids = vec![
                Cid::from_str("bafkreic6kcrue6ms42ykrisq6or24pbrubnyouvmgvk7ft73fjd4ynslxi")
                    .unwrap(),
                Cid::from_str("bafkreicvuc5rwwjqzix7saaia55du44qqsnphdugvjxlbe446mjmupekl4")
                    .unwrap(),
                Cid::from_str("bafkreiepxrkqexuff4vhc4vp6co73ubbp2vmskbwwazaihln6wws2z4wly")
                    .unwrap(),
            ];

            // Request many blocks
            let handles = (0..100)
                .into_iter()
                .map(|i| {
                    let requested = cids[i % cids.len()];
                    tokio::spawn({
                        let blockstore = Arc::clone(&blockstore);
                        async move {
                            (
                                requested,
                                blockstore.get(&requested).await.unwrap().unwrap(),
                            )
                        }
                    })
                })
                .collect::<Vec<_>>();

            // Validate if the blocks received are correct
            for handle in handles {
                let (requested_cid, block_bytes) = handle.await.expect("Panic in task");

                // Generate the CID form the bytes. That way we can check if the
                // block data returned is correct.
                let multihash = generate_multihash::<Sha256, _>(&block_bytes);
                let generated_cid = Cid::new_v1(RAW_CODE, multihash);

                assert_eq!(requested_cid, generated_cid);
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::{io::Cursor, path::Path};

    use crate::{test_utils::assert_buffer_eq, FileReader};

    #[tokio::test]
    async fn read_duplicated_blocks() {
        let mut loader = FileReader::from_path("tests/fixtures/car_v2/zero.car")
            .await
            .unwrap();
        let root = loader.roots().await.unwrap()[0];
        let mut out_check = Cursor::new(vec![1u8; 4096]);
        loader.copy_tree(&root, &mut out_check).await.unwrap();

        let expected = [0u8; 524288].as_slice();
        let inner = out_check.into_inner();
        let result = inner.as_slice();

        assert_buffer_eq!(expected, result);
    }

    async fn load_and_compare<P1, P2>(original: P1, path: P2)
    where
        P1: AsRef<Path>,
        P2: AsRef<Path>,
    {
        let original = tokio::fs::read(original).await.unwrap();
        let mut car = FileReader::from_path(path).await.unwrap();

        let mut out_buffer: Vec<u8> = vec![];
        let root = car.roots().await.unwrap()[0];
        car.copy_tree(&root, &mut out_buffer).await.unwrap();

        crate::test_utils::assert_buffer_eq!(original, &out_buffer);
    }

    #[tokio::test]
    async fn read_empty() {
        let mut car = FileReader::from_path("tests/fixtures/car_v2/empty.car")
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
