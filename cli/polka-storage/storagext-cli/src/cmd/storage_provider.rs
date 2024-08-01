use anyhow::bail;
use clap::Subcommand;
use primitives_proofs::RegisteredPoStProof;
use storagext::{storage_provider::StorageProviderClient, PolkaStorageConfig};
use subxt::ext::sp_core::crypto::Ss58Codec;
use url::Url;

use crate::deser::{ParseablePath, PreCommitSector, ProveCommitSector, SubmitWindowedPoStParams};

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
}

impl StorageProviderCommand {
    /// Run a `storage-provider` command.
    ///
    /// Requires the target RPC address and a keypair able to sign transactions.
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(
            node_rpc,
            address = account_keypair.account_id().to_ss58check()
        )
    )]
    pub async fn run<Keypair>(
        self,
        node_rpc: Url,
        account_keypair: Keypair,
    ) -> Result<(), anyhow::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let client = StorageProviderClient::new(node_rpc).await?;
        match self {
            StorageProviderCommand::RegisterStorageProvider {
                peer_id,
                post_proof,
            } => {
                let block_hash = client
                    .register_storage_provider(
                        &account_keypair,
                        peer_id.clone(),
                        post_proof,
                    )
                    .await?;
                tracing::info!(
                    "[{}] Successfully registered {}, seal: {:?} in Storage Provider Pallet",
                    block_hash,
                    peer_id,
                    post_proof
                );
            }
            StorageProviderCommand::PreCommit { pre_commit_sector } => {
                let sector_number = pre_commit_sector.sector_number;
                let block_hash = client
                    .pre_commit_sector(&account_keypair, pre_commit_sector.into())
                    .await?;

                tracing::info!(
                    "[{}] Successfully pre-commited sector {}.",
                    block_hash,
                    sector_number
                );
            }
            StorageProviderCommand::ProveCommit {
                prove_commit_sector,
            } => {
                let sector_number = prove_commit_sector.sector_number;
                let block_hash = client
                    .prove_commit_sector(&account_keypair, prove_commit_sector.into())
                    .await?;

                tracing::info!(
                    "[{}] Successfully proven sector {}.",
                    block_hash,
                    sector_number
                );
            }
            StorageProviderCommand::SubmitWindowedProofOfSpaceTime { windowed_post } => {
                let block_hash = client
                    .submit_windowed_post(&account_keypair, windowed_post.into())
                    .await?;

                tracing::info!("[{}] Successfully submitted proof.", block_hash,);
            }
        }
        Ok(())
    }
}
