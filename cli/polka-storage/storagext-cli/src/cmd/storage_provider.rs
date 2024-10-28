use std::time::Duration;

use clap::Subcommand;
use primitives_proofs::{RegisteredPoStProof, SectorNumber};
use storagext::{
    deser::DeserializablePath,
    multipair::MultiPairSigner,
    runtime::{
        runtime_types::pallet_storage_provider::sector::ProveCommitSector as RuntimeProveCommitSector,
        storage_provider::{events as SpEvents, Event as SpEvent},
        HashOfPsc, SubmissionResult,
    },
    types::storage_provider::{
        FaultDeclaration as SxtFaultDeclaration, ProveCommitSector as SxtProveCommitSector,
        RecoveryDeclaration as SxtRecoveryDeclaration,
        SectorPreCommitInfo as SxtSectorPreCommitInfo,
        SubmitWindowedPoStParams as SxtSubmitWindowedPoStParams,
        TerminationDeclaration as SxtTerminationDeclaration,
    },
    StorageProviderClientExt,
};
use url::Url;

use super::display_submission_result;
use crate::{missing_keypair_error, operation_takes_a_while, OutputFormat};

macro_rules! trace_submission_result {
    ($submission_result:expr, $format:expr $(,$par1:expr)*) => (
        if let Some(result) = $submission_result {
            if let Ok(events) = result {
                tracing::debug!(
                    $format,
                    events[0].hash,
                    $($par1),*
                );
                Ok(Some(Ok(events)))
            } else {
                Ok(Some(result))
            }
        } else {
            Ok(None)
        }
    )
}

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
        wait_for_finalization: bool,
        output_format: OutputFormat,
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
                        wait_for_finalization,
                        output_format,
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
        wait_for_finalization: bool,
        output_format: OutputFormat,
    ) -> Result<(), anyhow::Error>
    where
        Client: StorageProviderClientExt,
    {
        if wait_for_finalization {
            operation_takes_a_while();
        }

        match self {
            StorageProviderCommand::RegisterStorageProvider {
                peer_id,
                post_proof,
            } => {
                let opt_result = Self::register_storage_provider(
                    client,
                    account_keypair,
                    peer_id,
                    post_proof,
                    wait_for_finalization,
                )
                .await?;
                display_submission_result::<_, _>(opt_result, output_format)?;
            }
            StorageProviderCommand::PreCommit { pre_commit_sectors } => {
                let opt_result = Self::pre_commit(
                    client,
                    account_keypair,
                    pre_commit_sectors,
                    wait_for_finalization,
                )
                .await?;
                display_submission_result::<_, _>(opt_result, output_format)?;
            }
            StorageProviderCommand::ProveCommit {
                prove_commit_sectors,
            } => {
                let opt_result = Self::prove_commit(
                    client,
                    account_keypair,
                    prove_commit_sectors,
                    wait_for_finalization,
                )
                .await?;
                display_submission_result::<_, _>(opt_result, output_format)?;
            }
            StorageProviderCommand::SubmitWindowedProofOfSpaceTime { windowed_post } => {
                let opt_result = Self::submit_windowed_post(
                    client,
                    account_keypair,
                    windowed_post,
                    wait_for_finalization,
                )
                .await?;
                display_submission_result::<_, _>(opt_result, output_format)?;
            }
            StorageProviderCommand::DeclareFaults { faults } => {
                let opt_result =
                    Self::declare_faults(client, account_keypair, faults, wait_for_finalization)
                        .await?;
                display_submission_result::<_, _>(opt_result, output_format)?;
            }
            StorageProviderCommand::DeclareFaultsRecovered { recoveries } => {
                let opt_result = Self::declare_faults_recovered(
                    client,
                    account_keypair,
                    recoveries,
                    wait_for_finalization,
                )
                .await?;
                display_submission_result::<_, _>(opt_result, output_format)?;
            }
            StorageProviderCommand::TerminateSectors { terminations } => {
                let opt_result = Self::terminate_sectors(
                    client,
                    account_keypair,
                    terminations,
                    wait_for_finalization,
                )
                .await?;
                display_submission_result::<_, _>(opt_result, output_format)?;
            }
            _unsigned => unreachable!("unsigned commands should have been previously handled"),
        }

        Ok(())
    }

    async fn register_storage_provider<Client>(
        client: Client,
        account_keypair: MultiPairSigner,
        peer_id: String,
        post_proof: RegisteredPoStProof,
        wait_for_finalization: bool,
    ) -> Result<
        Option<SubmissionResult<HashOfPsc, SpEvent, SpEvents::StorageProviderRegistered>>,
        subxt::Error,
    >
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
            .await?;
        trace_submission_result!(
            submission_result,
            "[{}] Successfully registered {}, seal: {:?} in Storage Provider Pallet",
            peer_id,
            post_proof
        )
    }

    async fn pre_commit<Client>(
        client: Client,
        account_keypair: MultiPairSigner,
        pre_commit_sectors: Vec<SxtSectorPreCommitInfo>,
        wait_for_finalization: bool,
    ) -> Result<
        Option<SubmissionResult<HashOfPsc, SpEvent, SpEvents::SectorsPreCommitted>>,
        subxt::Error,
    >
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
            .await?;
        trace_submission_result!(
            submission_result,
            "[{}] Successfully pre-commited sectors {:?}.",
            sector_numbers
        )
    }

    async fn prove_commit<Client>(
        client: Client,
        account_keypair: MultiPairSigner,
        prove_commit_sectors: Vec<SxtProveCommitSector>,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<HashOfPsc, SpEvent, SpEvents::SectorsProven>>, subxt::Error>
    where
        Client: StorageProviderClientExt,
    {
        let (sector_numbers, prove_commit_sectors): (
            Vec<SectorNumber>,
            Vec<RuntimeProveCommitSector>,
        ) = prove_commit_sectors
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
            .await?;
        trace_submission_result!(
            submission_result,
            "[{}] Successfully proven sector {:?}.",
            sector_numbers
        )
    }

    async fn submit_windowed_post<Client>(
        client: Client,
        account_keypair: MultiPairSigner,
        windowed_post: SxtSubmitWindowedPoStParams,
        wait_for_finalization: bool,
    ) -> Result<
        Option<SubmissionResult<HashOfPsc, SpEvent, SpEvents::ValidPoStSubmitted>>,
        subxt::Error,
    >
    where
        Client: StorageProviderClientExt,
    {
        let submission_result = client
            .submit_windowed_post(
                &account_keypair,
                windowed_post.into(),
                wait_for_finalization,
            )
            .await?;
        trace_submission_result!(submission_result, "[{}] Successfully submitted proof.")
    }

    async fn declare_faults<Client>(
        client: Client,
        account_keypair: MultiPairSigner,
        faults: Vec<SxtFaultDeclaration>,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<HashOfPsc, SpEvent, SpEvents::FaultsDeclared>>, subxt::Error>
    where
        Client: StorageProviderClientExt,
    {
        let n_faults = faults.len();
        let submission_result = client
            .declare_faults(&account_keypair, faults, wait_for_finalization)
            .await?;
        trace_submission_result!(
            submission_result,
            "[{}] Successfully declared {} faults.",
            n_faults
        )
    }

    async fn declare_faults_recovered<Client>(
        client: Client,
        account_keypair: MultiPairSigner,
        recoveries: Vec<SxtRecoveryDeclaration>,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<HashOfPsc, SpEvent, SpEvents::FaultsRecovered>>, subxt::Error>
    where
        Client: StorageProviderClientExt,
    {
        let n_recoveries = recoveries.len();
        let submission_result = client
            .declare_faults_recovered(&account_keypair, recoveries, wait_for_finalization)
            .await?;
        trace_submission_result!(
            submission_result,
            "[{}] Successfully declared {} faults.",
            n_recoveries
        )
    }

    async fn terminate_sectors<Client>(
        client: Client,
        account_keypair: MultiPairSigner,
        terminations: Vec<SxtTerminationDeclaration>,
        wait_for_finalization: bool,
    ) -> Result<
        Option<SubmissionResult<HashOfPsc, SpEvent, SpEvents::SectorsTerminated>>,
        subxt::Error,
    >
    where
        Client: StorageProviderClientExt,
    {
        let n_terminations = terminations.len();
        let submission_result = client
            .terminate_sectors(&account_keypair, terminations, wait_for_finalization)
            .await?;
        trace_submission_result!(
            submission_result,
            "[{}] Successfully terminated {} sectors.",
            n_terminations
        )
    }
}
