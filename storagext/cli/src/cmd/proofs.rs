use std::time::Duration;

use clap::Subcommand;
use storagext::{
    clients::ProofsClientExt, multipair::MultiPairSigner, runtime::SubmissionResult,
    types::proofs::VerifyingKey, PolkaStorageConfig,
};
use url::Url;

use crate::{missing_keypair_error, OutputFormat};

#[derive(Debug, Subcommand)]
#[command(name = "proofs", about = "CLI Client to the Proofs Pallet", version)]
pub(crate) enum ProofsCommand {
    /// Set PoRep verifying key
    SetPorepVerifyingKey {
        /// Verifying key. Either hex encoded as string or, if prepended with @, a path to a file containing the key's raw bytes.
        #[arg(value_parser = VerifyingKey::value_parser)]
        verifying_key: VerifyingKey,
    },
    /// Set PoRep verifying key
    SetPostVerifyingKey {
        /// Verifying key. Either hex encoded as string or, if prepended with @, a path to a file containing the key's raw bytes.
        #[arg(value_parser = VerifyingKey::value_parser)]
        verifying_key: VerifyingKey,
    },
}

impl ProofsCommand {
    /// Run a `proofs` command.
    ///
    /// Requires the target RPC address and a keypair able to sign transactions.
    #[tracing::instrument(level = "info", skip(self, node_rpc), fields(node_rpc = node_rpc.as_str()))]
    pub async fn run(
        self,
        node_rpc: Url,
        account_keypair: Option<MultiPairSigner>,
        n_retries: u32,
        retry_interval: Duration,
        output_format: OutputFormat,
        wait_for_finalization: bool,
    ) -> Result<(), anyhow::Error> {
        let client = storagext::Client::new(node_rpc, n_retries, retry_interval).await?;

        let submission_result = match self {
            // NOTE: subcommand_negates_reqs does not work for this since it only negates the parents'
            // requirements, and the global arguments (keys) are at the grandparent level
            // https://users.rust-lang.org/t/clap-ignore-global-argument-in-sub-command/101701/8
            ProofsCommand::SetPorepVerifyingKey { verifying_key } => {
                let Some(account_keypair) = account_keypair else {
                    return Err(missing_keypair_error::<Self>().into());
                };

                Self::set_porep_verifying_key(
                    client,
                    account_keypair,
                    verifying_key,
                    wait_for_finalization,
                )
                .await?
            }
            ProofsCommand::SetPostVerifyingKey { verifying_key } => {
                let Some(account_keypair) = account_keypair else {
                    return Err(missing_keypair_error::<Self>().into());
                };

                Self::set_post_verifying_key(
                    client,
                    account_keypair,
                    verifying_key,
                    wait_for_finalization,
                )
                .await?
            }
        };

        let Some(submission_result) = submission_result else {
            return Ok(());
        };

        // This monstrosity first converts incoming events into a "generic" (subxt generated) event,
        // and then we extract only the Proofs events. We could probably extract this into a proper
        // iterator but the effort to improvement ratio seems low.
        let submission_results = submission_result
            .events
            .iter()
            .flat_map(|event| {
                event.map(|details| details.as_root_event::<storagext::runtime::Event>())
            })
            .filter_map(|event| match event {
                Ok(storagext::runtime::Event::Proofs(e)) => Some(Ok(e)),
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

    async fn set_porep_verifying_key<Client>(
        client: Client,
        account_keypair: MultiPairSigner,
        verifying_key: VerifyingKey,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Client: ProofsClientExt,
    {
        let submission_result = client
            .set_porep_verifying_key(&account_keypair, verifying_key, wait_for_finalization)
            .await?
            .inspect(|result| {
                tracing::debug!("[{}] PoRep Key successfully set", result.hash);
            });

        Ok(submission_result)
    }

    async fn set_post_verifying_key<Client>(
        client: Client,
        account_keypair: MultiPairSigner,
        verifying_key: VerifyingKey,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Client: ProofsClientExt,
    {
        let submission_result = client
            .set_post_verifying_key(&account_keypair, verifying_key, wait_for_finalization)
            .await?
            .inspect(|result| {
                tracing::debug!("[{}] PoSt Key successfully set", result.hash);
            });

        Ok(submission_result)
    }
}
