use std::future::Future;

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
            // NOTE: This is not the best way to implement this
            // see the source for .at_latest() for a possibly better version
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
        let mut block_stream = if wait_for_finalization {
            self.client.blocks().subscribe_finalized().await?
        } else {
            self.client.blocks().subscribe_best().await?
        };

        while let Some(block) = block_stream.next().await {
            let block = block?;
            if block.number() >= height {
                return Ok(());
            }
        }

        Err(subxt::Error::Rpc(
            subxt::error::RpcError::SubscriptionDropped,
        ))
    }
}
