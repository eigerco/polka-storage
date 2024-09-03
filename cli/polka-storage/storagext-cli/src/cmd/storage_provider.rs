use std::time::Duration;

use anyhow::bail;
use clap::Subcommand;
use primitives_proofs::RegisteredPoStProof;
use storagext::{clients::StorageProviderClient, FaultDeclaration, RecoveryDeclaration};
use url::Url;

use crate::{
    deser::{ParseablePath, PreCommitSector, ProveCommitSector, SubmitWindowedPoStParams},
    missing_keypair_error, operation_takes_a_while, MultiPairSigner,
};

fn parse_post_proof(src: &str) -> Result<RegisteredPoStProof, anyhow::Error> {
    let post_proof = match src {
        "2KiB" => RegisteredPoStProof::StackedDRGWindow2KiBV1P1,
        unknown => bail!("Unknown PoSt Proof type: {}", unknown),
    };

    Ok(post_proof)
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
    ) -> Result<(), anyhow::Error> {
        let client = StorageProviderClient::new(node_rpc, n_retries, retry_interval).await?;

        operation_takes_a_while();

        let Some(account_keypair) = account_keypair else {
            return Err(missing_keypair_error::<Self>().into());
        };
        let submission_result = match self {
            StorageProviderCommand::RegisterStorageProvider {
                peer_id,
                post_proof,
            } => {
                let submission_result = client
                    .register_storage_provider(&account_keypair, peer_id.clone(), post_proof)
                    .await?;
                tracing::debug!(
                    "[{}] Successfully registered {}, seal: {:?} in Storage Provider Pallet",
                    submission_result.hash,
                    peer_id,
                    post_proof
                );

                submission_result
            }
            StorageProviderCommand::PreCommit { pre_commit_sector } => {
                let sector_number = pre_commit_sector.sector_number;
                let submission_result = client
                    .pre_commit_sector(&account_keypair, pre_commit_sector.into())
                    .await?;
                tracing::debug!(
                    "[{}] Successfully pre-commited sector {}.",
                    submission_result.hash,
                    sector_number
                );

                submission_result
            }
            StorageProviderCommand::ProveCommit {
                prove_commit_sector,
            } => {
                let sector_number = prove_commit_sector.sector_number;
                let submission_result = client
                    .prove_commit_sector(&account_keypair, prove_commit_sector.into())
                    .await?;
                tracing::debug!(
                    "[{}] Successfully proven sector {}.",
                    submission_result.hash,
                    sector_number
                );

                submission_result
            }
            StorageProviderCommand::SubmitWindowedProofOfSpaceTime { windowed_post } => {
                let submission_result = client
                    .submit_windowed_post(&account_keypair, windowed_post.into())
                    .await?;
                tracing::debug!("[{}] Successfully submitted proof.", submission_result.hash);

                submission_result
            }
            StorageProviderCommand::DeclareFaults { faults } => {
                let submission_result = client.declare_faults(&account_keypair, faults).await?;
                tracing::debug!("[{}] Successfully declared faults.", submission_result.hash);

                submission_result
            }
            StorageProviderCommand::DeclareFaultsRecovered { recoveries } => {
                let submission_result = client
                    .declare_faults_recovered(&account_keypair, recoveries)
                    .await?;
                tracing::debug!("[{}] Successfully declared faults.", submission_result.hash);

                submission_result
            }
        };

        // This monstrosity first converts incoming events into a "generic" (subxt generated) event,
        // and then we extract only the Market events. We could probably extract this into a proper
        // iterator but the effort to improvement ratio seems low (for 2 pallets at least).
        for event in submission_result
            .events
            .iter()
            .flat_map(|event| {
                event.map(|details| details.as_root_event::<storagext::runtime::Event>())
            })
            .filter_map(|event| match event {
                Ok(storagext::runtime::Event::StorageProvider(e)) => Some(Ok(e)),
                Err(err) => Some(Err(err)),
                _ => None,
            })
        {
            let event = event?;
            println!("[{}] {}", submission_result.hash, event);
        }
        Ok(())
    }
}
