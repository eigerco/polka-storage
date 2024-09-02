use std::time::Duration;

use clap::Subcommand;
use storagext::clients::SystemClient;
use url::Url;

#[derive(Debug, Subcommand)]
#[command(name = "system", about = "System related actions", version)]
pub(crate) enum SystemCommand {
    /// Get current height
    GetHeight {
        /// Wait for finalized blocks only
        #[arg(long, default_value_t = false)]
        wait_for_finalization: bool,
    },
    /// Wait for a specific block height
    WaitForHeight {
        /// Block heights to wait for
        height: u64,

        /// Wait for finalized blocks only
        #[arg(long, default_value_t = false)]
        wait_for_finalization: bool,
    },
}

impl SystemCommand {
    /// Run a `system` command.
    ///
    /// Requires the target RPC address .
    #[tracing::instrument(level = "info", skip(self, node_rpc), fields(node_rpc = node_rpc.as_str()))]
    pub async fn run(
        self,
        node_rpc: Url,
        n_retries: u32,
        retry_interval: Duration,
    ) -> Result<(), anyhow::Error> {
        let client = SystemClient::new(node_rpc, n_retries, retry_interval).await?;

        match self {
            SystemCommand::GetHeight {
                wait_for_finalization,
            } => {
                let height = client.height(wait_for_finalization).await?;
                println!("Current height: {height:#?}");
            }
            SystemCommand::WaitForHeight {
                height,
                wait_for_finalization,
            } => {
                client
                    .wait_for_height(height, wait_for_finalization)
                    .await?;
                println!("Reached desired height");
            }
        };

        Ok(())
    }
}
