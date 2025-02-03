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
        let raw_pieces_dir = self
            .raw_pieces_dir
            .join(piece_cid.to_string())
            .with_extension("car");
        let mut raw_piece = File::open(&raw_pieces_dir).await.map_err(|_| {
            blockstore::Error::StoredDataError(format!("piece {:?} not found", raw_pieces_dir))
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
