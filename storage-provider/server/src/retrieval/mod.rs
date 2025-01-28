use std::{path::PathBuf, sync::Arc};

use blockstore::ProviderBlockstore;
use libp2p::Multiaddr;
use polka_storage_retrieval::Server;
use tokio_util::sync::CancellationToken;

use crate::{local_index_directory::Service, ServerError};

mod blockstore;

pub struct RetrievalServerConfig<I> {
    pub listen_address: Multiaddr,
    pub unsealed_sectors_dir: PathBuf,
    pub indexer: Arc<I>,
}

#[tracing::instrument(skip_all)]
pub async fn start_retrieval<I>(
    config: RetrievalServerConfig<I>,
    token: CancellationToken,
) -> Result<(), ServerError>
where
    I: Service + Send + Sync + 'static,
{
    // Blockstore used by the retrieval server provider
    let blockstore = Arc::new(ProviderBlockstore::new(
        config.unsealed_sectors_dir,
        config.indexer,
    ));

    // Setup & run the retrieval server
    let server = Server::new(blockstore)?;
    server
        .run(vec![config.listen_address], async move {
            token.cancelled_owned().await;
            tracing::trace!("shutting down the retrieval server");
        })
        .await?;

    Ok(())
}
