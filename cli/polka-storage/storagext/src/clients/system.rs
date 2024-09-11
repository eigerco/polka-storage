use std::{future::Future, time::Duration};

use tokio::time::sleep;

pub trait SystemClientExt {
    /// Get the current height of the chain.
    /// It returns latest non-finalized block.
    fn height(
        &self,
        wait_for_finalization: bool,
    ) -> impl Future<Output = Result<u64, subxt::Error>>;

    /// Wait for the chain to reach a specific height.
    fn wait_for_height(
        &self,
        height: u64,
        wait_for_finalization: bool,
    ) -> impl Future<Output = Result<(), subxt::Error>>;
}

impl SystemClientExt for crate::runtime::client::Client {
    /// Get the current height of the chain.
    /// It returns latest non-finalized block.
    async fn height(&self, wait_for_finalization: bool) -> Result<u64, subxt::Error> {
        let mut block_stream = if wait_for_finalization {
            self.client.blocks().subscribe_finalized().await?
        } else {
            self.client.blocks().subscribe_best().await?
        };

        let block = block_stream
            .next()
            .await
            .expect("there always exists a block on a running chain")?;

        Ok(block.header().number)
    }

    /// Wait for the chain to reach a specific height.
    async fn wait_for_height(
        &self,
        height: u64,
        wait_for_finalization: bool,
    ) -> Result<(), subxt::Error> {
        loop {
            let current_height = self.height(wait_for_finalization).await?;
            tracing::debug!("Current height: {current_height}");

            if current_height >= height {
                return Ok(());
            }

            sleep(Duration::from_secs(2)).await;
        }
    }
}
