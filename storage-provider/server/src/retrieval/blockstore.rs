use std::{path::Path, sync::Arc};

use blockstore::Error;

use crate::local_index_directory::Service;

/// The blockstore that reads blocks directly from unsealed sectors
pub struct ProviderBlockstore<I> {
    indexer: Arc<I>,
}

impl<I> ProviderBlockstore<I> {
    pub fn new<P>(unsealed_sectors_path: P, indexer: Arc<I>) -> Self
    where
        I: Service,
        P: AsRef<Path>,
    {
        Self { indexer }
    }
}

impl<I> blockstore::Blockstore for ProviderBlockstore<I>
where
    I: Send + Sync + 'static,
{
    async fn get<const S: usize>(
        &self,
        _cid: &cid::CidGeneric<S>,
    ) -> blockstore::Result<Option<Vec<u8>>> {
        // TODO: Find the sector that holds the requested cid
        // TODO: open a file handler to the sector file.
        // TODO: Read the blocks from the sector
        todo!()
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
