use std::time::Duration;

use tokio::time::sleep;

use crate::runtime;

pub struct SystemClient {
    client: crate::runtime::client::Client,
}

impl SystemClient {
    /// Create a new [`SystemClient`] from a target `rpc_address`.
    ///
    /// By default, this function does not support insecure URLs,
    /// to enable support for them, use the `insecure_url` feature.
    pub async fn new(rpc_address: impl AsRef<str>) -> Result<Self, subxt::Error> {
        Ok(Self {
            client: crate::runtime::client::Client::new(rpc_address).await?,
        })
    }

    /// Get the current height of the chain.
    pub async fn height(&self) -> Result<Option<u64>, subxt::Error> {
        let system_height_query = runtime::storage().system().number();
        self.client
            .client
            .storage()
            .at_latest()
            .await?
            .fetch(&system_height_query)
            .await
    }

    /// Wait for the chain to reach a specific height.
    pub async fn wait_for_height(&self, height: u64) -> Result<(), subxt::Error> {
        loop {
            let current_height = self.height().await?.unwrap_or_default();
            tracing::debug!("Current height: {current_height}");

            if current_height >= height {
                return Ok(());
            }

            sleep(Duration::from_secs(2)).await;
        }
    }
}
