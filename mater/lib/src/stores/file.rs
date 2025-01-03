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
    multicodec::{is_identity, SHA_256_CODE},
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
    // Inner store
    inner: RwLock<FileBlockstoreInner>,
}

/// Inner file store. Encapsulating state that is locked and used together.
struct FileBlockstoreInner {
    // Car file data store
    store: File,
    // The byte length of the CARv1 payload. This is used by the indexing, so we
    // know the locations of each blocks in the file.
    data_size: u64,
    // Index of blocks that will be appended to the file at the finalization.
    // Stored number is an offset that locates the first byte of the block
    // within the CARv1 payload. The offset is relative to the start of the
    // CARv1 payload.
    index: IndexMap<Cid, u64>,
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
        let written = v1::write_header(&mut file, &CarV1Header::new(roots)).await?;

        let inner = FileBlockstoreInner {
            store: file,
            data_size: written as u64,
            index: IndexMap::new(),
        };

        Ok(Self {
            inner: RwLock::new(inner),
        })
    }

    /// Check if the store contains a block with the cid. In case of IDENTITY
    /// CID it always returns true.
    async fn has(&self, cid: Cid) -> Result<bool, Error> {
        if is_identity(&cid).is_some() {
            return Ok(true);
        }

        Ok(self.inner.read().await.index.get(&cid).is_some())
    }

    /// Get specific block from the store. If the CID is an identity, the digest
    /// from the cid is returned.
    async fn get(&self, cid: Cid) -> Result<Option<Vec<u8>>, Error> {
        // If CID is an identity
        if let Some(data) = is_identity(&cid) {
            return Ok(Some(data.to_owned()));
        }

        // The lock is hold through out the method execution. That way we are
        // certain that the file is not used and we are moving the cursor back
        // to the correct place after the read.
        let mut inner = self.inner.write().await;

        // Get the index if exists
        let Some(index) = inner.index.get(&cid).copied() else {
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
    async fn put_keyed(&self, cid: &Cid, data: &[u8]) -> Result<(), Error> {
        if is_identity(&cid).is_some() {
            return Ok(());
        }

        // The lock is hold through out the method execution
        let mut inner = self.inner.write().await;

        // This is a current position of the writer. We save this to the indexer
        // so that we know where we wrote the current block.
        let current_position = inner.store.stream_position().await?;
        let index_location = current_position - CarV2Header::SIZE;

        // Write block
        let mut buffered_writer = BufWriter::new(&mut inner.store);
        let written = write_block(&mut buffered_writer, &cid, data).await?;
        buffered_writer.flush().await?;
        inner.data_size += written as u64;

        // Add current block to the index
        inner.index.insert(*cid, index_location);

        Ok(())
    }

    /// Finalize the blockstore by writing the CARv2 header, along with index
    /// for more efficient subsequent read.
    async fn finalize(self) -> Result<(), Error> {
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

        // Write the index
        inner
            .store
            .seek(SeekFrom::Start(header.index_offset))
            .await?;
        let count = inner.index.len() as u64;
        let entries = inner
            .index
            .into_iter()
            .map(|(cid, offset)| IndexEntry::new(cid.hash().digest().to_vec(), offset as u64))
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
    use blockstore::{Blockstore, Error};
    use ipld_core::cid::{Cid, CidGeneric};

    use crate::FileBlockstore;

    impl Blockstore for FileBlockstore {
        async fn get<const S: usize>(&self, cid: &CidGeneric<S>) -> Result<Option<Vec<u8>>, Error> {
            let cid = Cid::try_from(cid.to_bytes()).map_err(|_err| Error::CidTooLarge)?;

            self.get(cid)
                .await
                .map_err(|err| Error::FatalDatabaseError(err.to_string()))
        }

        async fn has<const S: usize>(&self, cid: &CidGeneric<S>) -> blockstore::Result<bool> {
            let cid = Cid::try_from(cid.to_bytes()).map_err(|_err| Error::CidTooLarge)?;

            self.has(cid)
                .await
                .map_err(|err| Error::FatalDatabaseError(err.to_string()))
        }

        async fn put_keyed<const S: usize>(
            &self,
            cid: &CidGeneric<S>,
            data: &[u8],
        ) -> Result<(), Error> {
            let cid = Cid::try_from(cid.to_bytes()).map_err(|_err| Error::CidTooLarge)?;

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
            self.finalize()
                .await
                .map_err(|err| Error::FatalDatabaseError(err.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{io::Cursor, path::PathBuf, str::FromStr};

    use ipld_core::cid::{multihash::Multihash, Cid};
    use tempfile::TempDir;
    use tokio::{
        fs::File,
        io::{AsyncReadExt, AsyncSeekExt},
    };

    use crate::{
        multicodec::{IDENTITY_CODE, RAW_CODE},
        CarV2Reader, Error, FileBlockstore,
    };

    /// Initialize a new blockstore
    async fn init_blockstore(roots: Vec<Cid>) -> Result<(TempDir, PathBuf, FileBlockstore), Error> {
        let tmp_dir = TempDir::new().unwrap();
        let blockstore_file_path = tmp_dir.path().join("blockstore.car");
        let blockstore = FileBlockstore::new(&blockstore_file_path, roots).await?;

        Ok((tmp_dir, blockstore_file_path, blockstore))
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
        assert_eq!(payload, content.as_slice());
    }

    #[tokio::test]
    async fn test_blockstore() {
        // Car file
        let original_archive = tokio::fs::read("tests/fixtures/car_v2/spaceglenda.car")
            .await
            .unwrap();

        let mut reader = CarV2Reader::new(Cursor::new(original_archive.clone()));
        reader.read_pragma().await.unwrap();
        let header = reader.read_header().await.unwrap();
        let v1_header = reader.read_v1_header().await.unwrap();

        let (_guard, blockstore_file, blockstore) = init_blockstore(v1_header.roots).await.unwrap();

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

        // Finalize blockstore
        blockstore.finalize().await.unwrap();

        // Load new archive file to memory
        let mut file = File::open(blockstore_file).await.unwrap();
        let mut new_archive = Vec::new();
        file.read_to_end(&mut new_archive).await.unwrap();

        // Compare both files
        assert_eq!(original_archive, new_archive);
    }

    #[tokio::test]
    async fn test_multiple() {}
}
