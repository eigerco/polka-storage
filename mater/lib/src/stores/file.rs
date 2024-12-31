use std::{io::SeekFrom, path::Path};

use indexmap::IndexMap;
use ipld_core::cid::Cid;
use sha2::{Digest, Sha256};
use tokio::{
    fs::File,
    io::{AsyncSeekExt, AsyncWriteExt, BufWriter},
    sync::{Mutex, RwLock},
};

use crate::{
    multicodec::SHA_256_CODE,
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
/// The blockstore should be closed once the putting blocks is finished. Upon
/// closing the blockstore, the index is written out to underlying file.
pub struct FileBlockstore {
    // Inner store
    inner: Mutex<FileBlockstoreInner>,
    // Index of blocks that will be appended to the file at the finalization
    index: RwLock<IndexMap<Cid, u64>>,
}

/// Inner file store. Encapsulating state that is locked and used together.
struct FileBlockstoreInner {
    // Car file data store
    store: File,
    // The byte length of the CARv1 payload. This is used by the indexing, so we
    // know the locations of each blocks in the file.
    data_size: u64,
    // Is true if the blockstore was finalized.
    is_finalized: bool,
}

impl FileBlockstore {
    /// Create a new blockstore. If file at the path already exists it is truncated.
    pub async fn new<P>(path: P, roots: Vec<Cid>) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        let mut file = File::options()
            .truncate(true)
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
            is_finalized: false,
        };

        Ok(Self {
            inner: Mutex::new(inner),
            index: RwLock::new(IndexMap::new()),
        })
    }

    async fn has(&self, cid: Cid) -> Result<bool, Error> {
        Ok(self.index.read().await.get(&cid).is_some())
    }

    /// Get specific block from the store
    async fn get(&self, cid: Cid) -> Result<Option<Vec<u8>>, Error> {
        // Get the index if exists
        let Some(index) = self.index.read().await.get(&cid).copied() else {
            return Ok(None);
        };

        // The lock is hold through out the method execution. That way we are
        // certain that the file is not used and we are moving the cursor back
        // to the correct place after the read.
        let mut inner = self.inner.lock().await;
        let current_cursor_location = inner.store.stream_position().await?;

        // Move cursor to the location of the block
        inner
            .store
            .seek(SeekFrom::Start(CarV2Header::SIZE + index))
            .await?;

        // Read block
        let (block_cid, block_data) = read_block(&mut inner.store).await?;
        debug_assert_eq!(block_cid, cid);

        // Move cursor back to the original position
        inner
            .store
            .seek(SeekFrom::Start(current_cursor_location))
            .await?;

        return Ok(Some(block_data));
    }

    /// Put a new block in the store
    async fn put(&self, cid: &Cid, data: &[u8]) -> Result<(), Error> {
        // Lock writer
        let mut inner = self.inner.lock().await;

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
        self.index.write().await.insert(*cid, index_location);

        Ok(())
    }

    /// Finalize this blockstore by writing the CARv2 header, along with
    /// flattened index for more efficient subsequent read.
    async fn finalize(self) -> Result<(), Error> {
        // Locked underlying file handler
        let mut inner = self.inner.lock().await;

        // The blockstore was already finalized
        if inner.is_finalized {
            return Ok(());
        }

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

        // Flatten and write the index
        inner
            .store
            .seek(SeekFrom::Start(header.index_offset))
            .await?;
        let index = self.index.read().await.clone();
        let count = index.len() as u64;
        let entries = index
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
        inner.is_finalized = true;

        Ok(())
    }
}

#[cfg(feature = "blockstore")]
impl blockstore::Blockstore for FileBlockstore {
    async fn get<const S: usize>(
        &self,
        cid: &CidGeneric<S>,
    ) -> Result<Option<Vec<u8>>, blockstore::Error> {
        let cid = Cid::try_from(cid.to_bytes()).map_err(|_err| blockstore::Error::CidTooLarge)?;

        self.get(cid)
            .await
            .map_err(|err| blockstore::Error::FatalDatabaseError(err.to_string()))
    }

    async fn has<const S: usize>(&self, cid: &CidGeneric<S>) -> blockstore::Result<bool> {
        let cid = Cid::try_from(cid.to_bytes()).map_err(|_err| blockstore::Error::CidTooLarge)?;

        self.has(cid)
            .await
            .map_err(|err| blockstore::Error::FatalDatabaseError(err.to_string()))
    }

    async fn put_keyed<const S: usize>(
        &self,
        cid: &CidGeneric<S>,
        data: &[u8],
    ) -> Result<(), blockstore::Error> {
        let cid = Cid::try_from(cid.to_bytes()).map_err(|_err| blockstore::Error::CidTooLarge)?;

        self.put(&cid, data)
            .await
            .map_err(|err| blockstore::Error::FatalDatabaseError(err.to_string()))
    }

    async fn remove<const S: usize>(&self, _cid: &CidGeneric<S>) -> Result<(), blockstore::Error> {
        unimplemented!("Operation not supported")
    }

    async fn close(self) -> Result<(), blockstore::Error> {
        self.finalize()
            .await
            .map_err(|err| blockstore::Error::FatalDatabaseError(err.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::{io::Cursor, path::PathBuf, str::FromStr};

    use tempfile::NamedTempFile;
    use tokio::{
        fs::File,
        io::{AsyncReadExt, AsyncSeekExt},
    };

    use crate::{CarV2Reader, Error, FileBlockstore};

    #[tokio::test]
    async fn test_blockstore() {
        // Car file
        let mut file =
            File::open(PathBuf::from_str("tests/fixtures/car_v2/spaceglenda.car").unwrap())
                .await
                .unwrap();
        let mut original_archive = Vec::new();
        file.read_to_end(&mut original_archive).await.unwrap();

        let mut reader = CarV2Reader::new(Cursor::new(original_archive.clone()));
        reader.read_pragma().await.unwrap();
        let header = reader.read_header().await.unwrap();

        let v1_header = reader.read_v1_header().await.unwrap();

        let blockstore_file_path = NamedTempFile::new().unwrap();
        let blockstore = FileBlockstore::new(&blockstore_file_path, v1_header.roots)
            .await
            .unwrap();

        loop {
            // NOTE(@jmg-duarte,22/05/2024): review this
            match reader.read_block().await {
                Ok((cid, data)) => {
                    // Add block to the store
                    blockstore.put(&cid, &data).await.unwrap();

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
                else_ => {
                    // With the length check above this branch should actually be unreachable
                    assert!(matches!(else_, Err(Error::IoError(_))));
                    break;
                }
            }
        }

        // Finalize blockstore
        blockstore.finalize().await.unwrap();

        // Load new archive file to memory
        let mut file = File::open(blockstore_file_path).await.unwrap();
        let mut new_archive = Vec::new();
        file.read_to_end(&mut new_archive).await.unwrap();

        // Compare both files
        assert_eq!(original_archive, new_archive);
    }
}
