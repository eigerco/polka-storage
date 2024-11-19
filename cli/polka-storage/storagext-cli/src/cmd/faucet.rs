use std::time::Duration;

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

                // This monstrosity first converts incoming events into a "generic" (subxt generated) event,
                // and then we extract only the Faucet events. We could probably extract this into a proper
                // iterator but the effort to improvement ratio seems low (for 2 pallets at least).
                let submission_results = submission_result
                    .events
                    .iter()
                    .flat_map(|event| {
                        event.map(|details| details.as_root_event::<storagext::runtime::Event>())
                    })
                    .filter_map(|event| match event {
                        Ok(storagext::runtime::Event::Faucet(e)) => Some(Ok(e)),
                        Err(err) => Some(Err(err)),
                        _ => None,
                    });
                for event in submission_results {
                    let event = event?;
                    let output = output_format.format(&event)?;
                    match output_format {
                        OutputFormat::Plain => println!("[{}] {}", submission_result.hash, output),
                        OutputFormat::Json => println!("{}", output),
                    }
                }
                Ok(())
            }
        }
    }
}
