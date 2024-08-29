use clap::Subcommand;
use storagext::clients::SystemClient;
use url::Url;

#[derive(Debug, Subcommand)]
#[command(name = "system", about = "System related actions", version)]
pub(crate) enum SystemCommand {
    /// Get current height
    GetHeight,
    /// Wait for a specific block height
    WaitForHeight {
        /// Block heights to wait for
        height: u64,
    },
}

impl SystemCommand {
    /// Run a `system` command.
    ///
    /// Requires the target RPC address .
    #[tracing::instrument(level = "info", skip(self, node_rpc), fields(node_rpc = node_rpc.as_str()))]
    pub async fn run(self, node_rpc: Url) -> Result<(), anyhow::Error> {
        let client = SystemClient::new(node_rpc).await?;

        match self {
            SystemCommand::GetHeight => match client.height().await? {
                Some(height) => println!("Current height: {height:#?}"),
                None => println!("No current height"),
            },
            SystemCommand::WaitForHeight { height } => {
                client.wait_for_height(height).await?;
                println!("Reached desired height");
            }
        };

        Ok(())
    }
}
