use std::time::Duration;

use clap::Subcommand;
use storagext::RandomnessClientExt;
use url::Url;

use crate::OutputFormat;

#[derive(Debug, Subcommand)]
#[command(
    name = "randomness",
    about = "CLI Client to the Randomness Pallet",
    version
)]
pub(crate) enum RandomnessCommand {
    /// Get random value from a block in hex format.
    GetRandomness {
        /// Block number
        block: storagext::BlockNumber,
    },
}

impl RandomnessCommand {
    /// Run a `randomness` command.
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
            // NOTE: subcommand_negates_reqs does not work for this since it only negates the parents'
            // requirements, and the global arguments (keys) are at the grandparent level
            // https://users.rust-lang.org/t/clap-ignore-global-argument-in-sub-command/101701/8
            RandomnessCommand::GetRandomness { block } => {
                if let Some(randomness) = client.get_randomness(block).await? {
                    let randomness = hex::encode(randomness);

                    tracing::debug!("Randomness for block number {block} is {randomness:?}");
                    println!("{}", output_format.format(&randomness)?);
                } else {
                    tracing::error!("Randomness is not available for this block number. If the block number was not yet mined, that means that the randomness was not yet generated. If the block number is in the past, the randomness might already be garbage collected.");
                }
            }
        };

        Ok(())
    }
}
