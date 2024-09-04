use std::time::Duration;

use clap::Subcommand;
use primitives_proofs::RegisteredPoStProof;
use storagext::{
    clients::StorageProviderClient, runtime::SubmissionResult, FaultDeclaration,
    PolkaStorageConfig, RecoveryDeclaration,
};
use url::Url;

use crate::{
    deser::{ParseablePath, PreCommitSector, ProveCommitSector, SubmitWindowedPoStParams},
    missing_keypair_error, operation_takes_a_while, MultiPairSigner, OutputFormat,
};

fn parse_post_proof(src: &str) -> Result<RegisteredPoStProof, String> {
    match src {
        "2KiB" => Ok(RegisteredPoStProof::StackedDRGWindow2KiBV1P1),
        unknown => Err(format!("Unknown PoSt Proof type: {}", unknown)),
    }
}

#[derive(Debug, Subcommand)]
#[command(
    name = "storage-provider",
    about = "CLI Client to the Storage Provider Pallet",
    version
)]
pub enum StorageProviderCommand {
    /// Register account as a Storage Provider, so it can perform duties in Storage Provider Pallet.
    #[command(name = "register")]
    RegisterStorageProvider {
        /// PeerId in Storage Provider P2P network, can be any String.
        peer_id: String,
        /// Proof of Space Time type.
        /// Can only be "2KiB" meaning `RegisteredPoStProof::StackedDRGWindow2KiBV1P1`.
        #[arg(long, value_parser = parse_post_proof, default_value = "2KiB")]
        post_proof: RegisteredPoStProof,
    },

    /// Retrieve all registered Storage Providers.
    RetrieveStorageProviders,

    /// Pre-commit sector containing deals, so they can be proven.
    /// If deals have been published and not pre-commited and proven, they'll be slashed by Market Pallet.
    PreCommit {
        #[arg(value_parser = <PreCommitSector as ParseablePath>::parse_json)]
        pre_commit_sector: PreCommitSector,
    },

    /// Proves sector that has been previously pre-committed.
    /// After proving, a deal in a sector is considered Active.
    ProveCommit {
        #[arg(value_parser = <ProveCommitSector as ParseablePath>::parse_json)]
        prove_commit_sector: ProveCommitSector,
    },

    /// Submit a Proof-of-SpaceTime (PoST).
    #[command(name = "submit-windowed-post")]
    SubmitWindowedProofOfSpaceTime {
        #[arg(value_parser = <SubmitWindowedPoStParams as ParseablePath>::parse_json)]
        windowed_post: SubmitWindowedPoStParams,
    },

    /// Declare faulty sectors.
    DeclareFaults {
        #[arg(value_parser = <Vec<FaultDeclaration> as ParseablePath>::parse_json)]
        // Needs to be fully qualified due to https://github.com/clap-rs/clap/issues/4626
        faults: std::vec::Vec<FaultDeclaration>,
    },

    /// Declare recovered faulty sectors.
    DeclareFaultsRecovered {
        #[arg(value_parser = <Vec<RecoveryDeclaration> as ParseablePath>::parse_json)]
        recoveries: std::vec::Vec<RecoveryDeclaration>,
    },
}

impl StorageProviderCommand {
    /// Run a `storage-provider` command.
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
    ) -> Result<(), anyhow::Error> {
        let client = StorageProviderClient::new(node_rpc, n_retries, retry_interval).await?;

        match self {
            // Only command that doesn't need a key.
            //
            // NOTE: subcommand_negates_reqs does not work for this since it only negates the parents'
            // requirements, and the global arguments (keys) are at the grandparent level
            // https://users.rust-lang.org/t/clap-ignore-global-argument-in-sub-command/101701/8
            StorageProviderCommand::RetrieveStorageProviders => {
                let storage_providers = client.retrieve_registered_storage_providers().await?;
                // Vec<String> does not implement Display and we can't implement it either
                // for now, this works well enough
                match output_format {
                    OutputFormat::Plain => {
                        println!("Registered Storage Providers: {:?}", storage_providers)
                    }
                    OutputFormat::Json => {
                        println!("{}", serde_json::to_string(&storage_providers)?)
                    }
                }
            }
            else_ => {
                let Some(account_keypair) = account_keypair else {
                    return Err(missing_keypair_error::<Self>().into());
                };
                else_
                    .with_keypair(client, account_keypair, output_format)
                    .await?;
            }
        };

        Ok(())
    }

    async fn with_keypair(
        self,
        client: StorageProviderClient,
        account_keypair: MultiPairSigner,
        output_format: OutputFormat,
    ) -> Result<(), anyhow::Error> {
        operation_takes_a_while();

        let submission_result = match self {
            StorageProviderCommand::RegisterStorageProvider {
                peer_id,
                post_proof,
            } => {
                Self::register_storage_provider(client, account_keypair, peer_id, post_proof)
                    .await?
            }
            StorageProviderCommand::PreCommit { pre_commit_sector } => {
                Self::pre_commit(client, account_keypair, pre_commit_sector).await?
            }
            StorageProviderCommand::ProveCommit {
                prove_commit_sector,
            } => Self::prove_commit(client, account_keypair, prove_commit_sector).await?,
            StorageProviderCommand::SubmitWindowedProofOfSpaceTime { windowed_post } => {
                Self::submit_windowed_post(client, account_keypair, windowed_post).await?
            }
            StorageProviderCommand::DeclareFaults { faults } => {
                Self::declare_faults(client, account_keypair, faults).await?
            }
            StorageProviderCommand::DeclareFaultsRecovered { recoveries } => {
                Self::declare_faults_recovered(client, account_keypair, recoveries).await?
            }
            _unsigned => unreachable!("unsigned commands should have been previously handled"),
        };

        // This monstrosity first converts incoming events into a "generic" (subxt generated) event,
        // and then we extract only the Market events. We could probably extract this into a proper
        // iterator but the effort to improvement ratio seems low (for 2 pallets at least).
        let submission_results = submission_result
            .events
            .iter()
            .flat_map(|event| {
                event.map(|details| details.as_root_event::<storagext::runtime::Event>())
            })
            .filter_map(|event| match event {
                Ok(storagext::runtime::Event::StorageProvider(e)) => Some(Ok(e)),
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

    async fn register_storage_provider(
        client: StorageProviderClient,
        account_keypair: MultiPairSigner,
        peer_id: String,
        post_proof: RegisteredPoStProof,
    ) -> Result<SubmissionResult<PolkaStorageConfig>, subxt::Error> {
        let submission_result = client
            .register_storage_provider(&account_keypair, peer_id.clone(), post_proof)
            .await?;
        tracing::debug!(
            "[{}] Successfully registered {}, seal: {:?} in Storage Provider Pallet",
            submission_result.hash,
            peer_id,
            post_proof
        );

        Ok(submission_result)
    }

    async fn pre_commit(
        client: StorageProviderClient,
        account_keypair: MultiPairSigner,
        pre_commit_sector: PreCommitSector,
    ) -> Result<SubmissionResult<PolkaStorageConfig>, subxt::Error> {
        let sector_number = pre_commit_sector.sector_number;
        let submission_result = client
            .pre_commit_sector(&account_keypair, pre_commit_sector.into())
            .await?;
        tracing::debug!(
            "[{}] Successfully pre-commited sector {}.",
            submission_result.hash,
            sector_number
        );

        Ok(submission_result)
    }

    async fn prove_commit(
        client: StorageProviderClient,
        account_keypair: MultiPairSigner,
        prove_commit_sector: ProveCommitSector,
    ) -> Result<SubmissionResult<PolkaStorageConfig>, subxt::Error> {
        let sector_number = prove_commit_sector.sector_number;
        let submission_result = client
            .prove_commit_sector(&account_keypair, prove_commit_sector.into())
            .await?;
        tracing::debug!(
            "[{}] Successfully proven sector {}.",
            submission_result.hash,
            sector_number
        );

        Ok(submission_result)
    }

    async fn submit_windowed_post(
        client: StorageProviderClient,
        account_keypair: MultiPairSigner,
        windowed_post: SubmitWindowedPoStParams,
    ) -> Result<SubmissionResult<PolkaStorageConfig>, subxt::Error> {
        let submission_result = client
            .submit_windowed_post(&account_keypair, windowed_post.into())
            .await?;
        tracing::debug!("[{}] Successfully submitted proof.", submission_result.hash);

        Ok(submission_result)
    }

    async fn declare_faults(
        client: StorageProviderClient,
        account_keypair: MultiPairSigner,
        faults: Vec<FaultDeclaration>,
    ) -> Result<SubmissionResult<PolkaStorageConfig>, subxt::Error> {
        let submission_result = client.declare_faults(&account_keypair, faults).await?;
        tracing::debug!("[{}] Successfully declared faults.", submission_result.hash);

        Ok(submission_result)
    }

    async fn declare_faults_recovered(
        client: StorageProviderClient,
        account_keypair: MultiPairSigner,
        recoveries: Vec<RecoveryDeclaration>,
    ) -> Result<SubmissionResult<PolkaStorageConfig>, subxt::Error> {
        let submission_result = client
            .declare_faults_recovered(&account_keypair, recoveries)
            .await?;
        tracing::debug!("[{}] Successfully declared faults.", submission_result.hash);

        Ok(submission_result)
    }
}
