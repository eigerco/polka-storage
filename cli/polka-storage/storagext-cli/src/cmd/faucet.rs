use std::time::Duration;

use anyhow::anyhow;
use clap::Subcommand;
use storagext::{FaucetClientExt, PolkaStorageConfig};
use url::Url;

use crate::OutputFormat;

#[derive(Debug, Subcommand)]
#[command(name = "faucet", about = "CLI Client to the Faucet Pallet", version)]
pub(crate) enum FaucetCommand {
    /// Drip funds into target account.
    Drip {
        /// Drip target's account ID.
        account_id: <PolkaStorageConfig as subxt::Config>::AccountId,
    },
}

impl FaucetCommand {
    /// Run a `faucet` command.
    ///
    /// Requires the target RPC address.
    #[tracing::instrument(level = "info", skip(self, node_rpc), fields(node_rpc = node_rpc.as_str()))]
    pub async fn run(
        self,
        node_rpc: Url,
        n_retries: u32,
        retry_interval: Duration,
        output_format: OutputFormat,
        wait_for_finalization: bool,
    ) -> Result<(), anyhow::Error> {
        let client = storagext::Client::new(node_rpc, n_retries, retry_interval).await?;

        match self {
            FaucetCommand::Drip { account_id } => {
                let submission_result = client.drip(account_id, wait_for_finalization).await?;

                let Some(submission_result) = submission_result else {
                    // Didn't wait for finalization
                    return Ok(());
                };

                // Grab `Dripped` event and convert into `::faucet::Event` so we have `Display` and `Serialize` implemented.
                let event: storagext::runtime::faucet::Event = submission_result
                    .events
                    .find_first::<storagext::runtime::faucet::events::Dripped>()?
                    .ok_or(anyhow!("Could not find expected Dripped event"))?
                    .into();

                let output = output_format.format(&event)?;
                match output_format {
                    OutputFormat::Plain => println!("[{}] {}", submission_result.hash, output),
                    OutputFormat::Json => println!("{}", output),
                }
                Ok(())
            }
        }
    }
}
