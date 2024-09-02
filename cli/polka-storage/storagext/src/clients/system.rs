use std::time::Duration;

use tokio::time::sleep;

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
    /// It returns latest non-finalized block.
    pub async fn height(&self) -> Result<u64, subxt::Error> {
        let mut best_stream = self.client.client.blocks().subscribe_best().await?;
        let block = best_stream
            .next()
            .await
            .expect("there always exists a block on a running chain")?;

        Ok(block.header().number)
    }

    /// Wait for the chain to reach a specific height.
    pub async fn wait_for_height(&self, height: u64) -> Result<(), subxt::Error> {
        loop {
            let current_height = self.height().await?;
            tracing::debug!("Current height: {current_height}");

            if current_height >= height {
                return Ok(());
            }

            sleep(Duration::from_secs(2)).await;
        }
    }
}
