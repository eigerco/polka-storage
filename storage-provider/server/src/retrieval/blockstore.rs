use std::{io::SeekFrom, path::PathBuf, sync::Arc};

use blockstore::{block::CidError, Error};
use cid::{Cid, CidGeneric};
use mater::CidExt;
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt},
};

use crate::indexer::local_index_directory::Service;

/// The blockstore that reads blocks directly from the raw pieces
pub struct ProviderBlockstore<I> {
    indexer: Arc<I>,
    raw_pieces_dir: PathBuf,
}

impl<I> ProviderBlockstore<I> {
    pub fn new<P>(raw_pieces_dir: P, indexer: Arc<I>) -> Self
    where
        I: Service,
        P: Into<PathBuf>,
    {
        Self {
            indexer,
            raw_pieces_dir: raw_pieces_dir.into(),
        }
    }
}

impl<I> blockstore::Blockstore for ProviderBlockstore<I>
where
    I: Service + Send + Sync + 'static,
{
    async fn get<const S: usize>(
        &self,
        cid: &cid::CidGeneric<S>,
    ) -> blockstore::Result<Option<Vec<u8>>> {
        // If CID is an identity
        if let Some(data) = cid.get_identity_data() {
            return Ok(Some(data.to_owned()));
        }

        let cid = to_blockstore_cid(cid)?;

        // Pieces containing the cid.
        let Ok(pieces) = self.indexer.pieces_containing_multihash(*cid.hash()) else {
            return Ok(None);
        };

        // We take the first piece that contains the multihash
        let Some(piece_cid) = pieces.first() else {
            return Ok(None);
        };

        // Get the offset of the block within the piece (CAR file)
        let block = self
            .indexer
            .get_offset_size(*piece_cid, *cid.hash())
            .map_err(|err| blockstore::Error::StoredDataError(err.to_string()))?;

        // Open the raw piece and read the block
        let raw_pieces_path = self
            .raw_pieces_dir
            .join(piece_cid.to_string())
            .with_extension("car");
        // TODO(@cernicc,04/02/2025): Currently we are opening the file for each
        // block. This can be optimized by holding a pool of open file handlers.
        let mut raw_piece = File::open(&raw_pieces_path).await.map_err(|_| {
            blockstore::Error::StoredDataError(format!("piece {:?} not found", raw_pieces_path))
        })?;
        raw_piece
            .seek(SeekFrom::Start(block.offset))
            .await
            .map_err(|_| {
                blockstore::Error::StoredDataError(
                    "error occurred while seeking the raw piece".into(),
                )
            })?;

        // Read block data
        let mut buffer = vec![0; block.size as usize];
        raw_piece.read_exact(&mut buffer).await.map_err(|err| {
            blockstore::Error::StoredDataError(format!(
                "error occurred while reading the raw piece: {}",
                err.to_string()
            ))
        })?;

        Ok(Some(buffer))
    }

    async fn put_keyed<const S: usize>(
        &self,
        _cid: &cid::CidGeneric<S>,
        _data: &[u8],
    ) -> blockstore::Result<()> {
        Err(Error::FatalDatabaseError(
            "put operation not supported".to_string(),
        ))
    }

    async fn remove<const S: usize>(&self, _cid: &cid::CidGeneric<S>) -> blockstore::Result<()> {
        Err(Error::FatalDatabaseError(
            "remove operation not supported".to_string(),
        ))
    }

    async fn close(self) -> blockstore::Result<()> {
        Ok(())
    }
}

/// Convert CID with the generic Multihash size to the CID with the specific
/// Multihash size that the underlying blockstore expects.
fn to_blockstore_cid<const S: usize>(cid: &CidGeneric<S>) -> Result<Cid, Error> {
    let digest_size = cid.hash().size() as usize;
    let hash = cid
        .hash()
        .resize::<64>()
        .map_err(|_| Error::CidError(CidError::InvalidMultihashLength(digest_size)))?;

    Ok(Cid::new(cid.version(), cid.codec(), hash).expect("we know cid is correct here"))
}

#[cfg(test)]
mod tests {
    use std::{path::Path, sync::Arc};

    use blockstore::Blockstore;
    use cid::{multihash::Multihash, Cid};
    use futures::{pin_mut, StreamExt};
    use mater::{stream_blocks_metadata, IDENTITY_CODE, RAW_CODE};
    use primitives::commitment::{CommP, Commitment};
    use tempfile::{tempdir, TempDir};
    use tokio::{fs::File, io::BufReader};

    use super::ProviderBlockstore;
    use crate::indexer::local_index_directory::rdb::{RocksDBLid, RocksDBStateStoreConfig};

    /// Initialize a new blockstore and index a given piece
    async fn init_blockstore<P>(location: P) -> (TempDir, ProviderBlockstore<RocksDBLid>)
    where
        P: AsRef<Path>,
    {
        // Dummy piece commitment
        let dummy_commitment = Commitment::<CommP>::from([0; 32]);

        // Index database
        let temp_dir = tempdir().unwrap();
        let db = Arc::new(
            RocksDBLid::new(RocksDBStateStoreConfig {
                path: temp_dir.path().into(),
            })
            .unwrap(),
        );

        // Move piece to the location used by the blockstore and rename the file to its pice_cid
        let raw_piece_path = temp_dir
            .path()
            .join(dummy_commitment.cid().to_string())
            .with_extension("car");
        tokio::fs::copy(&location, &raw_piece_path).await.unwrap();

        // Index the piece
        crate::indexer::tests::index_piece_util(Arc::clone(&db), dummy_commitment, raw_piece_path)
            .await;

        let blockstore = ProviderBlockstore::new(temp_dir.path(), db);
        (temp_dir, blockstore)
    }

    #[tokio::test]
    async fn test_get_identity_cid() {
        let (_guard, blockstore) =
            init_blockstore("tests/fixtures/spaceglenda_wrapped_v2.car").await;

        let payload = b"Hello World!";
        let multihash = Multihash::wrap(IDENTITY_CODE, payload).unwrap();
        let identity_cid = Cid::new_v1(RAW_CODE, multihash);

        let has_block = blockstore.has(&identity_cid).await.unwrap();
        assert!(has_block);

        let content = blockstore.get(&identity_cid).await.unwrap().unwrap();
        assert_eq!(payload.to_vec(), content);
    }

    #[tokio::test]
    async fn test_get_block() {
        let piece_path = "tests/fixtures/spaceglenda_wrapped_v2.car";
        let (_guard, blockstore) = init_blockstore(&piece_path).await;

        // Check if blocks are provided by the blockstore
        let file = File::open(piece_path).await.unwrap();
        let reader = BufReader::new(file);
        let blocks = stream_blocks_metadata(reader).await.unwrap();
        pin_mut!(blocks);

        while let Some(Ok(block)) = blocks.next().await {
            let blockstore_block = blockstore.get(&block.cid).await.unwrap().unwrap();
            assert!(!blockstore_block.is_empty());
        }
    }
}
