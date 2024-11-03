use std::time::Duration;

use clap::Subcommand;
use primitives_proofs::{RegisteredPoStProof, SectorNumber};
use storagext::{
    deser::DeserializablePath,
    multipair::MultiPairSigner,
    runtime::{
        runtime_types::pallet_storage_provider::sector::ProveCommitSector as RuntimeProveCommitSector,
        SubmissionResult,
    },
    types::storage_provider::{
        FaultDeclaration as SxtFaultDeclaration, ProveCommitSector as SxtProveCommitSector,
        RecoveryDeclaration as SxtRecoveryDeclaration,
        SectorPreCommitInfo as SxtSectorPreCommitInfo,
        SubmitWindowedPoStParams as SxtSubmitWindowedPoStParams,
        TerminationDeclaration as SxtTerminationDeclaration,
    },
    PolkaStorageConfig, StorageProviderClientExt,
};
use url::Url;

use crate::{missing_keypair_error, operation_takes_a_while, OutputFormat};

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
        #[arg(value_parser = <Vec<SxtSectorPreCommitInfo> as DeserializablePath>::deserialize_json)]
        pre_commit_sectors: std::vec::Vec<SxtSectorPreCommitInfo>,
    },

    /// Proves sector that has been previously pre-committed.
    /// After proving, a deal in a sector is considered Active.
    ProveCommit {
        #[arg(value_parser = <Vec<SxtProveCommitSector> as DeserializablePath>::deserialize_json)]
        prove_commit_sectors: std::vec::Vec<SxtProveCommitSector>,
    },

    /// Submit a Proof-of-SpaceTime (PoST).
    #[command(name = "submit-windowed-post")]
    SubmitWindowedProofOfSpaceTime {
        #[arg(value_parser = <SxtSubmitWindowedPoStParams as DeserializablePath>::deserialize_json)]
        windowed_post: SxtSubmitWindowedPoStParams,
    },

    /// Declare faulty sectors.
    DeclareFaults {
        #[arg(value_parser = <Vec<SxtFaultDeclaration> as DeserializablePath>::deserialize_json)]
        // Needs to be fully qualified due to https://github.com/clap-rs/clap/issues/4626
        faults: std::vec::Vec<SxtFaultDeclaration>,
    },

    /// Declare recovered faulty sectors.
    DeclareFaultsRecovered {
        #[arg(value_parser = <Vec<SxtRecoveryDeclaration> as DeserializablePath>::deserialize_json)]
        recoveries: std::vec::Vec<SxtRecoveryDeclaration>,
    },

    /// Terminate sectors.
    TerminateSectors {
        #[arg(value_parser = <Vec<SxtTerminationDeclaration> as DeserializablePath>::deserialize_json)]
        terminations: std::vec::Vec<SxtTerminationDeclaration>,
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
        wait_for_finalization: bool,
    ) -> Result<(), anyhow::Error> {
        let client = storagext::Client::new(node_rpc, n_retries, retry_interval).await?;

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
                    .with_keypair(
                        client,
                        account_keypair,
                        output_format,
                        wait_for_finalization,
                    )
                    .await?;
            }
        };

        Ok(())
    }

    async fn with_keypair<Client>(
        self,
        client: Client,
        account_keypair: MultiPairSigner,
        output_format: OutputFormat,
        wait_for_finalization: bool,
    ) -> Result<(), anyhow::Error>
    where
        Client: StorageProviderClientExt,
    {
        operation_takes_a_while();

        let submission_result = match self {
            StorageProviderCommand::RegisterStorageProvider {
                peer_id,
                post_proof,
            } => {
                Self::register_storage_provider(
                    client,
                    account_keypair,
                    peer_id,
                    post_proof,
                    wait_for_finalization,
                )
                .await?
            }
            StorageProviderCommand::PreCommit { pre_commit_sectors } => {
                Self::pre_commit(
                    client,
                    account_keypair,
                    pre_commit_sectors,
                    wait_for_finalization,
                )
                .await?
            }
            StorageProviderCommand::ProveCommit {
                prove_commit_sectors,
            } => {
                Self::prove_commit(
                    client,
                    account_keypair,
                    prove_commit_sectors,
                    wait_for_finalization,
                )
                .await?
            }
            StorageProviderCommand::SubmitWindowedProofOfSpaceTime { windowed_post } => {
                Self::submit_windowed_post(
                    client,
                    account_keypair,
                    windowed_post,
                    wait_for_finalization,
                )
                .await?
            }
            StorageProviderCommand::DeclareFaults { faults } => {
                Self::declare_faults(client, account_keypair, faults, wait_for_finalization).await?
            }
            StorageProviderCommand::DeclareFaultsRecovered { recoveries } => {
                Self::declare_faults_recovered(
                    client,
                    account_keypair,
                    recoveries,
                    wait_for_finalization,
                )
                .await?
            }
            StorageProviderCommand::TerminateSectors { terminations } => {
                Self::terminate_sectors(
                    client,
                    account_keypair,
                    terminations,
                    wait_for_finalization,
                )
                .await?
            }
            _unsigned => unreachable!("unsigned commands should have been previously handled"),
        };

        let Some(submission_result) = submission_result else {
            return Ok(());
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

    async fn register_storage_provider<Client>(
        client: Client,
        account_keypair: MultiPairSigner,
        peer_id: String,
        post_proof: RegisteredPoStProof,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Client: StorageProviderClientExt,
    {
        let submission_result = client
            .register_storage_provider(
                &account_keypair,
                peer_id.clone(),
                post_proof,
                wait_for_finalization,
            )
            .await?
            .inspect(|result| {
                tracing::debug!(
                    "[{}] Successfully registered {}, seal: {:?} in Storage Provider Pallet",
                    result.hash,
                    peer_id,
                    post_proof
                )
            });

        Ok(submission_result)
    }

    async fn pre_commit<Client>(
        client: Client,
        account_keypair: MultiPairSigner,
        pre_commit_sectors: Vec<SxtSectorPreCommitInfo>,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Client: StorageProviderClientExt,
    {
        let (sector_numbers, pre_commit_sectors): (Vec<SectorNumber>, Vec<SxtSectorPreCommitInfo>) =
            pre_commit_sectors
                .into_iter()
                .map(|s| (s.sector_number, s.into()))
                .unzip();

        let submission_result = client
            .pre_commit_sectors(&account_keypair, pre_commit_sectors, wait_for_finalization)
            .await?
            .inspect(|result| {
                tracing::debug!(
                    "[{}] Successfully pre-commited sectors {:?}.",
                    result.hash,
                    sector_numbers
                )
            });

        Ok(submission_result)
    }

    async fn prove_commit<Client>(
        client: Client,
        account_keypair: MultiPairSigner,
        prove_commit_sectors: Vec<SxtProveCommitSector>,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Client: StorageProviderClientExt,
    {
        let (sector_numbers, prove_commit_sectors): (Vec<SectorNumber>, Vec<SxtProveCommitSector>) =
            prove_commit_sectors
                .into_iter()
                .map(|s| {
                    let sector_number = s.sector_number;
                    (sector_number, s.into())
                })
                .unzip();
        let submission_result = client
            .prove_commit_sectors(
                &account_keypair,
                prove_commit_sectors,
                wait_for_finalization,
            )
            .await?
            .inspect(|result| {
                tracing::debug!(
                    "[{}] Successfully proven sector {:?}.",
                    result.hash,
                    sector_numbers
                )
            });

        Ok(submission_result)
    }

    async fn submit_windowed_post<Client>(
        client: Client,
        account_keypair: MultiPairSigner,
        windowed_post: SxtSubmitWindowedPoStParams,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Client: StorageProviderClientExt,
    {
        let submission_result = client
            .submit_windowed_post(
                &account_keypair,
                windowed_post.into(),
                wait_for_finalization,
            )
            .await?
            .inspect(|result| tracing::debug!("[{}] Successfully submitted proof.", result.hash));

        Ok(submission_result)
    }

    async fn declare_faults<Client>(
        client: Client,
        account_keypair: MultiPairSigner,
        faults: Vec<SxtFaultDeclaration>,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Client: StorageProviderClientExt,
    {
        let submission_result = client
            .declare_faults(&account_keypair, faults, wait_for_finalization)
            .await?
            .inspect(|result| tracing::debug!("[{}] Successfully declared faults.", result.hash));

        Ok(submission_result)
    }

    async fn declare_faults_recovered<Client>(
        client: Client,
        account_keypair: MultiPairSigner,
        recoveries: Vec<SxtRecoveryDeclaration>,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Client: StorageProviderClientExt,
    {
        let submission_result = client
            .declare_faults_recovered(&account_keypair, recoveries, wait_for_finalization)
            .await?
            .inspect(|result| tracing::debug!("[{}] Successfully declared faults.", result.hash));

        Ok(submission_result)
    }

    async fn terminate_sectors<Client>(
        client: Client,
        account_keypair: MultiPairSigner,
        terminations: Vec<SxtTerminationDeclaration>,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Client: StorageProviderClientExt,
    {
        let submission_result = client
            .terminate_sectors(&account_keypair, terminations, wait_for_finalization)
            .await?
            .inspect(|result| {
                tracing::debug!("[{}] Successfully terminated sectors.", result.hash)
            });

        Ok(submission_result)
    }
}
