use std::{io::SeekFrom, path::Path};

use indexmap::IndexMap;
use ipld_core::cid::Cid;
use sha2::{Digest, Sha256};
use tokio::{
    fs::File,
    io::{AsyncSeekExt, AsyncWriteExt, BufWriter},
    sync::RwLock,
};

use crate::{
    multicodec::{get_identity_data, SHA_256_CODE},
    v1::{self, read_block, write_block},
    v2::{self},
    CarV1Header, CarV2Header, Characteristics, Error, Index, IndexEntry, MultihashIndexSorted,
    SingleWidthIndex,
};

/// Implements a blockstore that stores blocks in a CARv2 format. Blocks put
/// into the blockstore can be read back once they are successfully written. The
/// blocks are written immediately, while the index is stored in memory and
/// updated incrementally.
///
/// The identity CIDs are not stored.
///
/// The blockstore should be closed once the putting blocks is finished. Upon
/// closing the blockstore, the index is written out to underlying file.
pub struct FileBlockstore {
    /// Original roots.
    roots: Vec<Cid>,
    /// Inner store
    inner: RwLock<FileBlockstoreInner>,
}

/// Inner file store. Encapsulating state that is locked and used together.
struct FileBlockstoreInner {
    /// Underlying data store in a CAR format
    store: File,
    /// The byte length of the CARv1 payload. This is used by the indexing, so we
    /// know the locations of each blocks in the file.
    data_size: u64,
    /// Index of blocks part of this store. This index is meant to be fast for
    /// the in memory lookups. The key represents a hash digest of the data
    /// block. Value is an offset that locates the first byte of the block
    /// within the CARv1 payload. The offset is relative to the start of the
    /// CARv1 payload.
    index: IndexMap<Vec<u8>, u64>,
}

impl FileBlockstore {
    /// Create a new blockstore. If file at the path already exists the error is thrown.
    pub async fn new<P>(path: P, roots: Vec<Cid>) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        if roots.is_empty() {
            return Err(crate::Error::EmptyRootsError);
        }

        let mut file = File::options()
            .create_new(true)
            .write(true)
            .read(true)
            .open(path)
            .await?;

        // Write headers
        v2::write_header(&mut file, &CarV2Header::default()).await?;
        let written = v1::write_header(&mut file, &CarV1Header::new(roots.clone())).await?;

        let inner = FileBlockstoreInner {
            store: file,
            data_size: written as u64,
            index: IndexMap::new(),
        };

        Ok(Self {
            inner: RwLock::new(inner),
            roots,
        })
    }

    /// Initialize the blockstore from the existing archive file. Error is
    /// thrown if the archive can't be read.
    ///
    /// Note: The underlying store is opened in read only mode. That means the
    /// returned blockstore can only be used to read an existing blocks.
    pub async fn from_existing<P>(path: P) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        let file = File::open(&path).await?;
        let mut reader = v2::Reader::new(file);

        // Read the headers
        reader.read_pragma().await?;
        let v2_header = reader.read_header().await?;
        let v1_header = reader.read_v1_header().await?;

        // This blockstore expects index to be used
        if v2_header.index_offset == 0 {
            return Err(Error::EmptyIndexError);
        }

        // Read the index
        let inner = reader.get_inner_mut();
        inner.seek(SeekFrom::Start(v2_header.index_offset)).await?;

        let mut index_map = IndexMap::new();
        match reader.read_index().await? {
            Index::IndexSorted(index) => {
                index.into_iter().flat_map(|a| a.entries).for_each(|index| {
                    index_map.insert(index.digest, index.offset);
                });
            }
            Index::MultihashIndexSorted(index) => {
                index
                    .into_iter()
                    .flat_map(|(_, index_sorted)| index_sorted.into_iter())
                    .flat_map(|a| a.entries)
                    .for_each(|index| {
                        index_map.insert(index.digest, index.offset);
                    });
            }
        }

        Ok(Self {
            roots: v1_header.roots,
            inner: RwLock::new(FileBlockstoreInner {
                store: File::open(path).await?,
                data_size: v2_header.data_size,
                index: index_map,
            }),
        })
    }

    /// Check if the store contains a block with the cid. In case of IDENTITY
    /// CID it always returns true.
    pub async fn has(&self, cid: Cid) -> Result<bool, Error> {
        if get_identity_data(&cid).is_some() {
            return Ok(true);
        }

        let digest = cid.hash().digest();
        Ok(self.inner.read().await.index.get(digest).is_some())
    }

    /// Get specific block from the store. If the CID is an identity, the digest
    /// from the cid is returned.
    pub async fn get(&self, cid: Cid) -> Result<Option<Vec<u8>>, Error> {
        // If CID is an identity
        if let Some(data) = get_identity_data(&cid) {
            return Ok(Some(data.to_owned()));
        }

        // The lock is held throughout the method execution. That way we are
        // certain that the file is not used and we are moving the cursor back
        // to the correct place after the read.
        let mut inner = self.inner.write().await;

        // Get the index if exists
        let digest = cid.hash().digest();
        let Some(index) = inner.index.get(digest).copied() else {
            return Ok(None);
        };

        // Move cursor to the location of the block
        inner
            .store
            .seek(SeekFrom::Start(CarV2Header::SIZE + index))
            .await?;

        // Read the lock
        let (block_cid, block_data) = read_block(&mut inner.store).await?;
        debug_assert_eq!(block_cid, cid);

        // Move cursor back to the position where we'll continue writing next blocks.
        let writing_position = CarV2Header::SIZE + inner.data_size;
        inner.store.seek(SeekFrom::Start(writing_position)).await?;

        return Ok(Some(block_data));
    }

    /// Put the new block in the store. The data integrity is not checked. We
    /// expect that the CID correctly represents the data being passed. In case
    /// of the identity CID, nothing is written to the store.
    pub async fn put_keyed(&self, cid: &Cid, data: &[u8]) -> Result<(), Error> {
        if get_identity_data(&cid).is_some() {
            return Ok(());
        }

        // The lock is hold through out the method execution
        let mut inner = self.inner.write().await;

        // This is a current position of the writer. We save this to the indexer
        // so that we know where we wrote the current block.
        let current_position = inner.store.stream_position().await?;
        let index_location = current_position.saturating_sub(CarV2Header::SIZE);

        // Write block
        let mut buffered_writer = BufWriter::new(&mut inner.store);
        let written = write_block(&mut buffered_writer, &cid, data).await?;
        buffered_writer.flush().await?;
        inner.data_size += written as u64;

        // Add current block to the index
        let digest = cid.hash().digest();
        inner.index.insert(digest.to_vec(), index_location);

        Ok(())
    }

    /// Finalize the blockstore by writing the CARv2 header, along with index
    /// for more efficient subsequent read. If roots are passed they overwrite
    /// the ones used to initialize the store.
    pub async fn finalize(self, new_roots: Option<Vec<Cid>>) -> Result<(), Error> {
        // Owned inner value
        let mut inner = self.inner.into_inner();

        // Correct CARv2 header
        let header = CarV2Header {
            characteristics: Characteristics::EMPTY,
            data_offset: CarV2Header::SIZE,
            data_size: inner.data_size,
            index_offset: CarV2Header::SIZE + inner.data_size,
        };

        // Write correct CARv2 header
        inner.store.rewind().await?;
        v2::write_header(&mut inner.store, &header).await?;

        // Overwrite CARv1 header if new roots were provided.
        if let Some(new_roots) = new_roots {
            // If the length is different we would overwrite part of the content.
            if self.roots.len() != new_roots.len() {
                return Err(Error::WrongNumberOfRoots);
            }

            v1::write_header(&mut inner.store, &CarV1Header::new(new_roots)).await?;
        }

        // Write the index
        inner
            .store
            .seek(SeekFrom::Start(header.index_offset))
            .await?;
        let count = inner.index.len() as u64;
        let entries = inner
            .index
            .into_iter()
            .map(|(digest, offset)| IndexEntry::new(digest, offset))
            .collect();
        let index = Index::MultihashIndexSorted(MultihashIndexSorted::from_single_width(
            SHA_256_CODE,
            SingleWidthIndex::new(Sha256::output_size() as u32, count, entries).into(),
        ));
        v2::write_index(&mut inner.store, &index).await?;

        // Flush underlying writer
        inner.store.flush().await?;

        Ok(())
    }
}

#[cfg(feature = "blockstore")]
mod blockstore {
    use blockstore::{block::CidError, Blockstore, Error};
    use ipld_core::cid::{Cid, CidGeneric};

    use crate::FileBlockstore;

    impl Blockstore for FileBlockstore {
        async fn get<const S: usize>(&self, cid: &CidGeneric<S>) -> Result<Option<Vec<u8>>, Error> {
            let cid = to_blockstore_cid(cid)?;

            self.get(cid)
                .await
                .map_err(|err| Error::FatalDatabaseError(err.to_string()))
        }

        async fn has<const S: usize>(&self, cid: &CidGeneric<S>) -> blockstore::Result<bool> {
            let cid = to_blockstore_cid(cid)?;

            self.has(cid)
                .await
                .map_err(|err| Error::FatalDatabaseError(err.to_string()))
        }

        async fn put_keyed<const S: usize>(
            &self,
            cid: &CidGeneric<S>,
            data: &[u8],
        ) -> Result<(), Error> {
            let cid = to_blockstore_cid(cid)?;

            self.put_keyed(&cid, data)
                .await
                .map_err(|err| Error::FatalDatabaseError(err.to_string()))
        }

        async fn remove<const S: usize>(&self, _cid: &CidGeneric<S>) -> Result<(), Error> {
            Err(Error::FatalDatabaseError(
                "remove operation not supported".to_string(),
            ))
        }

        async fn close(self) -> Result<(), Error> {
            self.finalize(None)
                .await
                .map_err(|err| Error::FatalDatabaseError(err.to_string()))
        }
    }

    /// Convert CID with the generic Multihash size to the CID with the specific
    /// Multihash size that the underlying blockstore expects.
    fn to_blockstore_cid<const S: usize>(cid: &CidGeneric<S>) -> Result<Cid, Error> {
        let hash = cid.hash().resize::<64>().map_err(|err| {
            Err(Error::CidError(CidError::InvalidMultihashLength(
                cid.hash().size(),
            )))
        });

        Ok(Cid::new(cid.version(), cid.codec(), hash))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::{Path, PathBuf},
        str::FromStr,
        sync::Arc,
    };

    use ipld_core::cid::{multihash::Multihash, Cid};
    use sha2::Sha256;
    use tempfile::TempDir;
    use tokio::{
        fs::File,
        io::{AsyncReadExt, AsyncSeekExt},
    };

    use crate::{
        multicodec::{generate_multihash, IDENTITY_CODE, RAW_CODE},
        test_utils::assert_buffer_eq,
        CarV2Reader, Error, FileBlockstore,
    };

    /// Initialize a new blockstore
    async fn init_blockstore(roots: Vec<Cid>) -> Result<(TempDir, PathBuf, FileBlockstore), Error> {
        let tmp_dir = TempDir::new().unwrap();
        let blockstore_file_path = tmp_dir.path().join("blockstore.car");
        let blockstore = FileBlockstore::new(&blockstore_file_path, roots).await?;

        Ok((tmp_dir, blockstore_file_path, blockstore))
    }

    /// Load blockstore from the existing archive.
    async fn load_from_existing_archive<P>(
        path: P,
    ) -> Result<(TempDir, PathBuf, FileBlockstore), Error>
    where
        P: AsRef<Path>,
    {
        let file = File::open(path).await.unwrap();
        let mut reader = CarV2Reader::new(file);
        reader.read_pragma().await.unwrap();
        let header = reader.read_header().await?;
        let v1_header = reader.read_v1_header().await?;

        let (guard, blockstore_file, blockstore) = init_blockstore(v1_header.roots).await?;

        loop {
            match reader.read_block().await {
                Ok((cid, data)) => {
                    // Add block to the store
                    blockstore.put_keyed(&cid, &data).await.unwrap();

                    // Check if the blockstore has a new block
                    assert!(blockstore.has(cid).await.unwrap());

                    // Get the same block back and check if it's the same
                    let block = blockstore.get(cid).await.unwrap().unwrap();
                    assert_eq!(block, data);

                    // Kinda hacky, but better than doing a seek later on
                    let position = reader.get_inner_mut().stream_position().await.unwrap();
                    let data_end = header.data_offset + header.data_size;
                    if position >= data_end {
                        break;
                    }
                }
                _ => {
                    unreachable!("the length check should avoid this from being reached");
                }
            }
        }

        Ok((guard, blockstore_file, blockstore))
    }

    #[tokio::test]
    async fn test_file_exists_error() {
        let existing_path = PathBuf::from_str("tests/fixtures/car_v2/spaceglenda.car").unwrap();
        let blockstore = FileBlockstore::new(&existing_path, vec![]).await;

        assert!(blockstore.is_err());
    }

    #[tokio::test]
    async fn test_no_roots_error() {
        let tmp_dir = TempDir::new().unwrap();
        let blockstore_file_path = tmp_dir.path().join("blockstore.car");
        let blockstore = FileBlockstore::new(&blockstore_file_path, vec![]).await;

        assert!(blockstore.is_err());
    }

    #[tokio::test]
    async fn test_identity_cid() {
        let arbitrary_root_cid =
            Cid::from_str("bafkreiczsrdrvoybcevpzqmblh3my5fu6ui3tgag3jm3hsxvvhaxhswpyu").unwrap();
        let (_guard, _file, blockstore) = init_blockstore(vec![arbitrary_root_cid]).await.unwrap();

        let payload = b"Hello World!";
        let multihash = Multihash::wrap(IDENTITY_CODE, payload).unwrap();
        let identity_cid = Cid::new_v1(RAW_CODE, multihash);

        let has_block = blockstore.has(identity_cid).await.unwrap();
        assert!(has_block);

        let content = blockstore.get(identity_cid).await.unwrap().unwrap();
        assert_buffer_eq!(&payload, &content);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn test_parallel_readers() {
        let (_guard, _file, blockstore) =
            load_from_existing_archive("tests/fixtures/car_v2/spaceglenda.car")
                .await
                .unwrap();
        let blockstore = Arc::new(blockstore);

        // CIDs of the content blocks that the spaceglenda.car contains. We are
        // only looking at the raw content so that our validation is easier later.
        let cids = vec![
            Cid::from_str("bafkreic6kcrue6ms42ykrisq6or24pbrubnyouvmgvk7ft73fjd4ynslxi").unwrap(),
            Cid::from_str("bafkreicvuc5rwwjqzix7saaia55du44qqsnphdugvjxlbe446mjmupekl4").unwrap(),
            Cid::from_str("bafkreiepxrkqexuff4vhc4vp6co73ubbp2vmskbwwazaihln6wws2z4wly").unwrap(),
        ];

        // Request many blocks
        let handles = (0..100)
            .into_iter()
            .map(|i| {
                let requested = cids[i % cids.len()];
                tokio::spawn({
                    let blockstore = Arc::clone(&blockstore);
                    async move { (requested, blockstore.get(requested).await.unwrap().unwrap()) }
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

    #[tokio::test]
    async fn test_blockstore_finalization() {
        let original_archive_path = "tests/fixtures/car_v2/spaceglenda.car";
        let original_archive = tokio::fs::read(original_archive_path).await.unwrap();

        let (_guard, blockstore_file, blockstore) =
            load_from_existing_archive(original_archive_path)
                .await
                .unwrap();

        // We are finalizing the blockstore so that the correct index and
        // headers are written out.
        blockstore.finalize(None).await.unwrap();

        // Load new archive file to memory
        let mut file = File::open(blockstore_file).await.unwrap();
        let mut new_archive = Vec::new();
        file.read_to_end(&mut new_archive).await.unwrap();

        // Compare both contents
        assert_buffer_eq!(&original_archive, &new_archive);
    }

    #[tokio::test]
    async fn test_blockstore_from_existing() {
        // Loaded blockstore
        let blockstore = FileBlockstore::from_existing("tests/fixtures/car_v2/spaceglenda.car")
            .await
            .unwrap();

        // Writing a new block should fail because the Blockstore is in read only mode.
        let payload = b"Hello World!";
        let cid = Cid::new_v1(RAW_CODE, generate_multihash::<Sha256, _>(payload));
        let writing_result = blockstore.put_keyed(&cid, payload).await;
        assert!(writing_result.is_err());

        // Blockstore should have a block
        let request_cid =
            Cid::from_str("bafkreic6kcrue6ms42ykrisq6or24pbrubnyouvmgvk7ft73fjd4ynslxi").unwrap();
        assert!(blockstore.has(request_cid).await.unwrap());

        // We should be able to get a block
        let reading_result = blockstore.get(request_cid).await.unwrap().unwrap();

        let generated_cid = Cid::new_v1(RAW_CODE, generate_multihash::<Sha256, _>(reading_result));
        assert_eq!(request_cid, generated_cid);

        // Finalization should fail
        assert!(blockstore.finalize(None).await.is_err());
    }
}
